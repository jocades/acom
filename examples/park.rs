use std::{
    thread::{self, Thread},
    time::{Duration, Instant},
};

struct Parker {
    me: Thread,
}

impl Parker {
    fn new() -> Self {
        Self {
            me: thread::current(),
        }
    }

    fn unparker(&self) -> Unparker {
        Unparker {
            who: self.me.clone(),
        }
    }

    fn park(&self) {
        assert_eq!(self.me.id(), thread::current().id());
        thread::park();
    }
}

struct Unparker {
    who: Thread,
}

impl Unparker {
    fn unpark(&self) {
        self.who.unpark();
    }
}

fn main() {
    let parker = Parker::new();
    let unparker = parker.unparker();

    thread::spawn(move || {
        println!("Thread spawned, waiting to unpark...");
        thread::sleep(Duration::from_secs(5));
        unparker.unpark();
    });

    let th = thread::spawn(move || {
        parker.park();
    });

    th.join().unwrap();

    // let now = Instant::now();
    // println!("Parking current thread");
    // parker.park();
    // println!("Unparked current thread; waited {:?}", now.elapsed());
}
