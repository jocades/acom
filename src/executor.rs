#![allow(unused)]

use std::{
    alloc::Layout,
    marker::PhantomData,
    pin::Pin,
    sync::{
        atomic::AtomicUsize,
        mpsc::{Receiver, Sender, channel},
    },
    task::{Context, Poll, Waker},
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
        let future = Box::pin(fut);
        let raw = RawTask::allocate(future);
        let task = Task { raw };
        QUEUE.with(|(tx, _)| tx.send(task).unwrap());
    }

    fn run(&self) {
        QUEUE.with(|(_, rx)| {
            while let Ok(task) = rx.recv() {
                task.poll();
                break;
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

struct RawTask<F, O> {
    header: *mut Header,
    future: *mut F,
    output: *mut O,
}

impl<F, O> Copy for RawTask<F, O> {}
impl<F, O> Clone for RawTask<F, O> {
    fn clone(&self) -> Self {
        *self
    }
}

struct RawTaskOffsets {
    future: usize,
    output: usize,
}

impl<F, O> RawTask<F, O>
where
    F: Future<Output = O>,
{
    fn compute_layout() -> (Layout, RawTaskOffsets) {
        let lay_header = Layout::new::<Header>();
        let lay_f = Layout::new::<F>();
        let lay_o = Layout::new::<O>();

        let layout = lay_header;
        let (_, future) = layout.extend(lay_f).unwrap();
        let (layout, output) = layout.extend(lay_o).unwrap();

        (layout, RawTaskOffsets { future, output })
    }

    fn arrange(p: *const (), offset: &RawTaskOffsets) -> Self {
        unsafe {
            Self {
                header: p as *mut Header,
                future: p.byte_add(offset.future) as *mut F,
                output: p.byte_add(offset.output) as *mut O,
            }
        }
    }

    fn allocate(future: F) -> *const () {
        println!("allocate fut");
        let (layout, offset) = Self::compute_layout();

        unsafe {
            let p = std::alloc::alloc(layout) as *const ();
            let raw = Self::arrange(p, &offset);

            println!("write header");
            raw.header.write(Header {
                state: AtomicUsize::new(0),
                vtable: &RawTaskVTable { poll: Self::poll },
            });

            raw.future.write(future);
            println!("write future");
            // (raw.byte_add(offset.future) as *mut F).write(future);

            println!("ret");
            p
        }
    }

    fn from_ptr(p: *const ()) -> Self {
        let (_, offset) = Self::compute_layout();
        Self::arrange(p, &offset)
    }

    fn poll(p: *const ()) {
        println!("polling!");
        let raw = Self::from_ptr(p);
        unsafe {
            let waker = Waker::noop();
            let mut cx = Context::from_waker(&waker);

            match F::poll(Pin::new_unchecked(&mut *raw.future), &mut cx) {
                Poll::Ready(v) => {}
                Poll::Pending => {}
            }
        };
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
