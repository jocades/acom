#![allow(unused)]

use std::{
    pin::Pin,
    task::{Context, Waker},
};

struct Guard {
    ptr: *const (),
}

struct Foo {
    one: i32,
    two: i32,
    closure: *const dyn Fn(),
    fun: fn(*const Self),
}

pub fn wtf() {
    let closure = || println!("closure!");

    fn fun(foo: *const Foo) {
        unsafe {
            println!("fun! {}", (*foo).one);
        };
    }

    let boxed = Box::new(Foo {
        one: 23,
        two: 45,
        closure: &closure,
        fun,
    });
    let task = Guard {
        ptr: Box::into_raw(boxed) as *const (),
    };
    unsafe {
        let ptr = task.ptr as *const i32;
        let one = (task.ptr.byte_add(0)) as *const i32;
        assert_eq!(std::mem::size_of::<usize>(), std::mem::size_of::<u64>());
        let two = (task.ptr as *const i32).add(usize::MAX) as *const i32;
        println!(
            "future = {} {} one = {} {} two = {} {}",
            ptr as usize, *ptr, one as usize, *one, two as usize, *two
        );

        println!("size_of fun = {}", std::mem::size_of::<&dyn Fn()>());
        let fun_ptr = (*(task.ptr as *const Foo)).closure;
        (*fun_ptr)();

        ((*task.ptr.cast::<Foo>()).fun)(task.ptr as *const Foo);
    };
}

pub fn into_pin() {
    struct Task<T> {
        future: *mut dyn Future<Output = T>,
    }

    impl<T> Copy for Task<T> {}
    impl<T> Clone for Task<T> {
        fn clone(&self) -> Self {
            *self
        }
    }

    let fut = async { 1 + 1 };

    unsafe {
        let ptr = Box::into_raw(Box::new(fut));
        let task = Task { future: ptr };

        let mut pinned_fut = Pin::new_unchecked(&mut *task.future);
        let mut cx = Context::from_waker(Waker::noop());
        let poll = pinned_fut.as_mut().poll(&mut cx);

        // let mut pinned = Pin::new_unchecked(&mut *ptr);

        // Box::from_raw()

        // dbg!(poll);
    };
}

fn main() {}
