#![allow(unused)]
use std::{
    marker::PhantomData,
    mem,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};

use tracing::{debug, info, instrument, trace};

use crate::task::{RawTask, Task};

struct LocalExecutor<'a> {
    queue: Receiver<Task>,
    sender: Sender<Task>,

    // Invariant `'a` lifetime.
    _marker: PhantomData<&'a ()>,
}

impl<'a> LocalExecutor<'a> {
    pub fn new() -> Self {
        let (sender, queue) = channel();
        Self {
            queue,
            sender,
            _marker: PhantomData,
        }
    }

    pub fn spawn<F, O>(&self, fut: F)
    where
        F: Future<Output = O>,
    {
        let future = Box::pin(fut);
        let tx = self.sender.clone();

        let schedule = |task| {
            trace!("schedule callback");
            tx.send(task).unwrap();
        };

        // let schedule = |_| trace!("callback!");
        let raw = RawTask::allocate(future, schedule);
        let task = Task { raw };
        task.schedule();
    }

    fn run(&self) {
        while let Ok(task) = self.queue.recv() {
            task.poll();
        }
    }
}

pub fn test() {
    let exec = LocalExecutor::new();

    let mut a = 1;
    let b = &mut a;

    exec.spawn(async move {
        let c = b;
        debug!("increment &mut a");
        yield_now().await;
    });

    exec.run();
    debug!("{a}");
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
                trace!("pending");
            }
            Poll::Ready(v) => {
                trace!("ready");
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
            trace!("yield_now.poll()");
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
