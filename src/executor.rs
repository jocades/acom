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

// fn scheduler(task: Task) {}

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
        F: Future<Output = O> + 'a,
    {
        let tx = self.sender.clone();
        let schedule = move |task| {
            trace!("schedule callback");
            tx.send(task).unwrap();
        };

        let task = Task {
            raw: if mem::size_of::<F>() > 2048 {
                RawTask::allocate(Box::pin(fut), schedule)
            } else {
                RawTask::allocate(fut, schedule)
            },
        };
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
        *c += 1;
        crate::future::yield_now().await;
    });

    exec.run();
    debug!("{a}");
}
