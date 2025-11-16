#![allow(unused)]
use std::{
    cell::UnsafeCell,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

// If the sender dropped an there is no value, err;
const SEND_DROP: usize = 0b01;

// If receiver dropped and sender tries to send, err;
const RECV_DROP: usize = 0b10;

struct Shared<T> {
    status: AtomicUsize,
    value: UnsafeCell<Option<T>>,
}

unsafe impl<T> Send for Shared<T> {}
unsafe impl<T> Sync for Shared<T> {}

struct Sender<T> {
    shared: Arc<Shared<T>>,
}

struct Receiver<T> {
    shared: Arc<Shared<T>>,
}

#[derive(Debug)]
struct SendError;

impl<T> Sender<T> {
    fn send(self, value: T) -> Result<(), SendError> {
        if self.shared.status.load(Ordering::Acquire) & RECV_DROP != 0 {
            return Err(SendError);
        }
        unsafe { *self.shared.value.get() = Some(value) };
        Ok(())
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        println!("drop sender");
        self.shared.status.fetch_or(SEND_DROP, Ordering::Release);
    }
}

#[derive(Debug)]
struct RecvError;

impl<T> Receiver<T> {
    fn recv(self) -> Result<T, RecvError> {
        while self.shared.status.load(Ordering::Relaxed) & SEND_DROP == 0 {
            std::hint::spin_loop();
        }

        unsafe { &mut *self.shared.value.get() }
            .take()
            .ok_or(RecvError)
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        println!("drop receiver");
        self.shared.status.fetch_or(RECV_DROP, Ordering::Release);
    }
}

fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Shared {
        status: AtomicUsize::new(0),
        value: UnsafeCell::new(None),
    });

    (
        Sender {
            shared: shared.clone(),
        },
        Receiver { shared },
    )
}

fn main() {
    let (tx, rx) = channel::<i32>();

    // drop(rx);

    let th = thread::spawn(|| {
        println!("[thread] entered");
        thread::sleep(std::time::Duration::from_secs(1));
        let _tx = tx;
        // println!("[thread] sending");
        // tx.send(1).unwrap();
    });

    let res = rx.recv();
    // th.join().unwrap();
    println!("result = {res:?}");
}
