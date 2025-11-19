use std::cell::OnceCell;
use std::marker::PhantomData;
use std::mem;
use std::pin::Pin;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::task::{Context, Poll, Waker};

use tracing::{debug, trace};

use crate::{
    reactor::Reactor,
    task::{RawTask, Task},
};

pub struct Executor<'a> {
    queue: Receiver<Task>,

    // Invariant `'a` lifetime.
    _marker: PhantomData<&'a ()>,
}

thread_local! {
    static SENDER: OnceCell<Sender<Task>> = OnceCell::new();
}

impl<'a> Executor<'a> {
    pub fn new() -> Self {
        let (sender, queue) = channel();
        SENDER.with(move |s| s.set(sender).expect("already init"));
        Self {
            queue,
            _marker: PhantomData,
        }
    }

    pub fn run(&self) {
        let mut events: [libc::kevent; Reactor::MAX_EVENTS] = unsafe { mem::zeroed() };

        loop {
            while let Ok(task) = self.queue.try_recv() {
                task.poll();
            }

            debug!("wait for events");
            let nev = Reactor::get().wait(&mut events);
            for ev in &events[..nev] {
                debug!(?ev);
                let waker = unsafe { Box::from_raw(ev.udata as *mut Waker) };
                waker.wake();
            }
        }
    }
}

pub fn spawn<'a, F, O>(fut: F)
where
    F: Future<Output = O> + 'a,
{
    trace!("spawn");
    SENDER.with(|s| {
        let tx = s.get().unwrap().clone();
        let schedule = move |task| {
            trace!("callback");
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
    });
}

use std::time::{Duration, Instant};

/// Asynchronously sleep for the specified duration. Yields the current task
/// and resumes it after the duration has elapsed.
pub async fn sleep(dur: Duration) {
    struct Sleep {
        duration: Duration,
        when: Instant,
        registered: bool,
    }

    impl Future for Sleep {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            if Instant::now() >= self.when {
                return Poll::Ready(());
            }

            if !self.registered {
                Reactor::get()
                    .wake_on_timer(self.duration, cx.waker().clone())
                    .unwrap();
                self.registered = true;
            }

            Poll::Pending
        }
    }

    Sleep {
        duration: dur,
        when: Instant::now() + dur,
        registered: false,
    }
    .await;
}
