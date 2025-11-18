#![allow(unused)]

use std::{
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Waker},
};

const SCHEDULED: usize = 1;
const CLOSED: usize = 1 << 3;

const REF_PAD: usize = 8;
const REF_COUNT: usize = 1 << REF_PAD;

fn dbg_state(s: usize) {
    println!(
        "scheduled = {} ref_count = {}",
        s & SCHEDULED != 0,
        s >> REF_PAD
    );
}

fn main() {
    let mut s = AtomicUsize::new(SCHEDULED | REF_COUNT);
    // dbg_state(s);
    let new = s.fetch_sub(REF_COUNT, Ordering::Relaxed) - REF_COUNT;
    // s -= REF_COUNT;
    dbg_state(new);
}
