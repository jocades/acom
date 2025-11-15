#![allow(unused)]

use std::{
    alloc::Layout,
    marker::PhantomData,
    pin::Pin,
    sync::{
        atomic::AtomicUsize,
        mpsc::{Receiver, Sender, channel},
    },
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};

thread_local! {
    static QUEUE: (Sender<Task>, Receiver<Task>) = channel()
}

struct LocalExecutor<'a> {
    // Invariant `'a` lifetime.
    _marker: PhantomData<&'a ()>,
}

impl<'a> LocalExecutor<'a> {
    pub const fn new() -> Self {
        Self {
            // queue: VecDeque::new(),
            _marker: PhantomData,
        }
    }

    pub fn spawn<F, O>(&mut self, fut: F)
    where
        F: Future<Output = O> + 'a,
    {
        QUEUE.with(|(tx, _)| {
            let future = Box::pin(fut);
            let schedule = |task| {
                println!("schedule me!");
                tx.send(task).unwrap();
            };
            let raw = RawTask::allocate(future, schedule);
            let task = Task { raw };
            println!("send task");
            tx.send(task).unwrap();
        });
    }

    fn run(&self) {
        QUEUE.with(|(_, rx)| {
            while let Ok(task) = rx.recv() {
                task.poll();
            }
        });
    }
}

struct Task {
    raw: *const (),
}

macro_rules! header {
    ($raw:expr) => {
        (*($raw as *const Header))
    };
}

impl Task {
    fn poll(&self) {
        unsafe { (header!(self.raw).vtable.poll)(self.raw) };
    }
}

struct RawTaskVTable {
    poll: unsafe fn(*const ()),
}

struct Header {
    state: AtomicUsize,
    vtable: &'static RawTaskVTable,
}

struct RawTask<F, O, S> {
    header: *mut Header,
    future: *mut F,
    output: *mut O,
    schedule: *mut S,
}

impl<F, O, S> Copy for RawTask<F, O, S> {}
impl<F, O, S> Clone for RawTask<F, O, S> {
    fn clone(&self) -> Self {
        *self
    }
}

struct RawTaskOffsets {
    future: usize,
    output: usize,
    schedule: usize,
}

impl<F, O, S> RawTask<F, O, S>
where
    F: Future<Output = O>,
    S: Fn(Task),
{
    fn layout() -> (Layout, RawTaskOffsets) {
        let lay_header = Layout::new::<Header>();
        let lay_f = Layout::new::<F>();
        let lay_o = Layout::new::<O>();
        let lay_s = Layout::new::<S>();

        let layout = lay_header;
        let (layout, future) = layout.extend(lay_f).unwrap();
        let (layout, output) = layout.extend(lay_o).unwrap();
        let (layout, schedule) = layout.extend(lay_s).unwrap();

        // println!("{layout:?} {future} {output} {schedule}");

        (
            layout,
            RawTaskOffsets {
                future,
                output,
                schedule,
            },
        )
    }

    fn arrange(p: *const (), offset: &RawTaskOffsets) -> Self {
        unsafe {
            Self {
                header: p as *mut Header,
                future: p.byte_add(offset.future) as *mut F,
                output: p.byte_add(offset.output) as *mut O,
                schedule: p.byte_add(offset.schedule) as *mut S,
            }
        }
    }

    fn allocate(future: F, schedule: S) -> *const () {
        println!("allocate fut");
        let (layout, offset) = Self::layout();

        unsafe {
            let p = std::alloc::alloc(layout) as *const ();
            let raw = Self::arrange(p, &offset);

            println!("write header");
            raw.header.write(Header {
                state: AtomicUsize::new(54),
                vtable: &RawTaskVTable { poll: Self::poll },
            });

            println!("write future");
            raw.future.write(future);

            println!("write callback");
            raw.schedule.write(schedule);

            println!("ret");
            p
        }
    }

    fn from_ptr(p: *const ()) -> Self {
        // println!("from_ptr");
        let (_, offset) = Self::layout();
        Self::arrange(p, &offset)
    }

    fn test(p: *const ()) {
        unsafe {
            let header = p as *const Header;
            let state = &(*header).state;
            println!("state = {state:?}");

            let header = p as *const Header;
            let this = p as *const Self;
            println!("header = {} this = {}", header as usize, this as usize);
            // let state = &(*(*p.cast::<Self>()).header).state;
            // println!("state = {state:?}");

            // let s = this.byte_add(30) as *const S;
            // let task = Task { raw: p, id: 32 };
            // (*s)(task);

            println!("schedule = {:#x}", (*p.cast::<Self>()).schedule as usize);
            let raw = Self::from_ptr(p);
            println!("schedule = {:#x}", raw.schedule as usize);
            let task = Task { raw: p };
            (*raw.schedule)(task);
            // raw.schedule.read()(task);
            // let task = Task { raw: p, id: 32 };
            // (*raw.schedule)(task);
        }
    }

    fn poll(p: *const ()) {
        // Self::test(p);
        println!("polling!");
        let raw = Self::from_ptr(p);
        unsafe {
            // let waker = Waker::noop();
            let waker = Waker::from_raw(RawWaker::new(p, &Self::RAW_WAKER_VTABLE));
            let mut cx = Context::from_waker(&waker);

            match F::poll(Pin::new_unchecked(&mut *raw.future), &mut cx) {
                Poll::Ready(v) => {}
                Poll::Pending => {}
            }
        };
    }

    const RAW_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
        Self::clone_waker,
        Self::wake,
        Self::wake_by_ref,
        Self::drop_waker,
    );

    unsafe fn clone_waker(p: *const ()) -> RawWaker {
        println!("waker clone");
        todo!()
    }

    unsafe fn wake(p: *const ()) {
        println!("waker wake");
    }

    unsafe fn wake_by_ref(p: *const ()) {
        println!("waker wake_by_ref");
        unsafe {
            let this = Self::from_ptr(p);
            let task = Task { raw: p };
            (*this.schedule)(task);
        }
    }

    unsafe fn drop_waker(p: *const ()) {
        println!("waker drop")
    }
}

pub fn works() {
    let mut exec = LocalExecutor::new();

    let mut a = 1;
    let b = &mut a;

    exec.spawn(async move {
        let c = b;
        println!("increment &mut a");
        *c += 1;
        yield_now().await;
    });

    exec.run();
    println!("{a}");
}

macro_rules! pin {
    ($x:ident) => {
        let mut $x = core::pin::pin!($x);
    };
}

pub fn spin_on<F: Future>(fut: F) -> F::Output {
    let mut cx = Context::from_waker(Waker::noop());

    pin!(fut);

    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Pending => {
                println!("pending");
            }
            Poll::Ready(v) => {
                println!("ready");
                return v;
            }
        }
    }
}

async fn yield_now() {
    struct YieldNow(bool);

    impl Future for YieldNow {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            println!("yield_now.poll()");
            if !self.0 {
                self.0 = true;
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            Poll::Ready(())
        }
    }

    YieldNow(false).await
}

pub fn test() {
    let f1 = async { 1 };
    let f2 = async {
        yield_now().await;
        2
    };
    dbg!(spin_on(f1));
    dbg!(spin_on(f2));
}
