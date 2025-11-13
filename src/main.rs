#![allow(unused)]

use std::{
    pin::{Pin, pin},
    ptr::null_mut,
    sync::{
        Arc,
        atomic::{
            AtomicUsize,
            Ordering::{Acquire, Relaxed, Release, SeqCst},
        },
    },
    task::{Wake, Waker},
    thread,
};

macro_rules! sc {
    ($fn:ident$args:tt) => {{
        let r = unsafe { libc::$fn$args };
        if r < 0 { Err(std::io::Error::last_os_error()) } else { Ok(r) }
    }};
}

fn main() {
    acom::executor::test();
}
