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

fn g<F: Future>(fut: F) {
    dbg!(std::mem::size_of::<F>());
}

fn drop_guard() {
    println!("top");
    let mut n = &mut 1;
    {
        println!("begin_scope");
        DropGuard(|| *n += 1);
        println!("end_scope");
    }
    println!("{n}");
    println!("bottom");
}

fn main() {
    executor::works();
    // executor::test();
}
