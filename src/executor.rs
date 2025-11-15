use std::{
    alloc::Layout,
    collections::VecDeque,
    marker::PhantomData,
    pin::Pin,
    sync::atomic::AtomicUsize,
    task::{Context, Poll, Waker},
};

struct LocalExecutor<'a> {
    queue: VecDeque<Task>,
    // Invariant `'a` lifetime.
    _marker: PhantomData<&'a ()>,
}

impl<'a> LocalExecutor<'a> {
    pub const fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            _marker: PhantomData,
        }
    }

    pub fn spawn<F>(&mut self, fut: F)
    where
        F: Future<Output = ()> + 'a,
    {
        let future = Box::pin(fut);
        let raw = RawTask::allocate(future);
        self.queue.push_back(Task { raw });
        // let task = Task { raw };
        // task.poll();
    }

    fn run(&mut self) {
        while let Some(task) = self.queue.pop_front() {
            task.poll();
            // let waker = Waker::noop();
            // let mut cx = Context::from_waker(&waker);

            // match fut.poll(&mut cx) {
            //     Poll::Ready(_) => {}
            //     Poll::Pending => {}
            // };
        }
    }
}

struct Task {
    raw: *const (),
}

impl Task {
    fn poll(&self) {
        // unsafe { ((*(self.raw as *const Header)).vtable.poll)(self.raw) };
        unsafe { ((*self.raw.cast::<Header>()).vtable.poll)(self.raw) };
    }
}

struct RawTaskVTable {
    poll: unsafe fn(*const ()),
}

struct Header {
    state: AtomicUsize,
    vtable: &'static RawTaskVTable,
}

struct RawTask<F> {
    header: *const Header,
    future: *mut F,
}

impl<F> Copy for RawTask<F> {}
impl<F> Clone for RawTask<F> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<F> RawTask<F>
where
    F: Future<Output = ()>,
{
    fn allocate(future: F) -> *const () {
        println!("allocate fut");
        let lay_header = Layout::new::<Header>();
        let lay_fut = Layout::new::<F>();

        let (layout, offset_fut) = lay_header.extend(lay_fut).unwrap();
        println!("layout = {layout:?}");

        unsafe {
            let raw = std::alloc::alloc(layout) as *mut Header;

            println!("write header");
            raw.write(Header {
                state: AtomicUsize::new(0),
                vtable: &RawTaskVTable { poll: Self::poll },
            });

            println!("write future");
            (raw.byte_add(offset_fut) as *mut F).write(future);

            println!("ret");
            raw as *const ()
        }
    }

    fn from_ptr(p: *const ()) -> Self {
        todo!()
        /* unsafe {
            Self {
                future: (*(p as *const Self)).future as *mut F,
            }
        } */
    }

    fn poll(p: *const ()) {
        println!("polling!");
        /* let raw = Self::from_ptr(p);
        unsafe {
            let waker = Waker::noop();
            let mut cx = Context::from_waker(&waker);

            match <F as Future>::poll(Pin::new_unchecked(&mut *raw.future), &mut cx) {
                Poll::Ready(v) => {}
                Poll::Pending => {}
            }
        }; */
    }
}

pub fn works() {
    let mut exec = LocalExecutor::new();

    let mut a = 1;
    let b = &mut a;
    *b += 1;

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
