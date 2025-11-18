use std::{
    cell::UnsafeCell,
    sync::{
        Arc,
        atomic::{
            AtomicBool,
            Ordering::{Acquire, Relaxed, Release},
        },
    },
    thread,
};

use acom::parallel::Parallel;

struct Mutex<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

unsafe impl<T> Send for Mutex<T> {}
unsafe impl<T> Sync for Mutex<T> {}

const LOCKED: bool = true;
const UNLOCKED: bool = false;

impl<T> Mutex<T> {
    fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(UNLOCKED),
            value: UnsafeCell::new(value),
        }
    }

    #[allow(unused)]
    fn with_lock<R>(&self, f: impl Fn(&mut T) -> R) -> R {
        while self
            .locked
            .compare_exchange_weak(UNLOCKED, LOCKED, Acquire, Relaxed)
            .is_err()
        {
            while self.locked.load(Relaxed) == LOCKED {
                thread::yield_now();
            }
            thread::yield_now();
        }
        let ret = f(unsafe { &mut *self.value.get() });
        self.locked.store(UNLOCKED, Release);
        ret
    }

    fn lock(&self) -> MutexGuard<'_, T> {
        while self
            .locked
            .compare_exchange_weak(UNLOCKED, LOCKED, Acquire, Relaxed)
            .is_err()
        {
            while self.locked.load(Relaxed) == LOCKED {
                std::hint::spin_loop();
            }
        }
        MutexGuard { lock: self }
    }
}

struct MutexGuard<'a, T> {
    lock: &'a Mutex<T>,
}

impl<T> std::ops::Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> std::ops::DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(UNLOCKED, Release);
    }
}

fn main() {
    let mutex = Arc::new(Mutex::new(0));

    Parallel::new()
        .each_with(&mutex, 0..10, |m, _| {
            for _ in 0..1_000_000 {
                // m.with_lock(|x| *x += 1);
                let mut guard = m.lock();
                *guard += 1;
            }
        })
        .run_unordered();

    assert_eq!(unsafe { *mutex.value.get() }, 10 * 1_000_000);
}
