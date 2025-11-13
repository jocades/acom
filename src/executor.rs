use std::{
    collections::VecDeque,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Wake, Waker},
};

struct LocalExecutor {
    queue: VecDeque<Rc<Task>>,
}

impl LocalExecutor {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    // pub fn spawn<F: Future<Output = ()>>(fut: F) {
    //     let task = Task {
    //         future: Box::pin(fut),
    //     };
    // }
}

struct Task {
    future: Pin<Box<dyn Future<Output = ()>>>,
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
