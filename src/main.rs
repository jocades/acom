#![allow(unused)]

use std::{
    cell::UnsafeCell,
    marker::PhantomData,
    pin::{Pin, pin},
    ptr::null_mut,
    sync::{
        Arc,
        atomic::{
            AtomicBool, AtomicPtr, AtomicUsize,
            Ordering::{Acquire, Relaxed, Release, SeqCst},
        },
    },
    task::{Wake, Waker},
    thread,
};

use acom::{DropGuard, executor, parallel::Parallel};
use tracing::{debug, trace};

fn g<F: Future>(fut: F) {
    dbg!(std::mem::size_of::<F>());
}

fn drop_guard() {
    trace!("top");
    let mut n = &mut 1;
    {
        trace!("begin_scope");
        DropGuard(|| *n += 1);
        trace!("end_scope");
    }
    trace!("{n}");
    trace!("bottom");
}

fn main() {
    acom::setup_logging();
    executor::test();
}
