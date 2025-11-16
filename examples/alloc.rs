use std::alloc::{GlobalAlloc, Layout, System, alloc, dealloc, handle_alloc_error};
use std::ptr::NonNull;
use std::sync::atomic::Ordering;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

struct Counter;

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ret = unsafe { System.alloc(layout) };
        if !ret.is_null() {
            ALLOCATED.fetch_add(layout.size(), Relaxed);
        }
        ret
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe {
            System.dealloc(ptr, layout);
        }
        ALLOCATED.fetch_sub(layout.size(), Relaxed);
    }
}

#[global_allocator]
static A: Counter = Counter;

fn how_many() {
    println!("allocated bytes = {}", ALLOCATED.load(Ordering::Acquire));
}

fn call<F: Fn()>(f: F) {
    dbg!(std::mem::size_of::<F>());
    how_many();
}

fn main() {
    println!("allocated bytes before main: {}", ALLOCATED.load(Relaxed));
    ALLOCATED.store(0, Relaxed);
    how_many();

    // let int: Box<i32> = Box::new(23);
    // how_many();
    // drop(int);
    // how_many();

    // let caputre_me = "hello".to_string();
    // let f = || println!("{caputre_me}");
    // call(f);
    // drop(caputre_me);
    // f();

    struct Foo<T, F> {
        t: T,
        f: F,
    }

    impl<T, F: Fn()> Foo<T, F> {
        fn allocate(t: T, f: F) -> NonNull<Self> {
            let lay_a = dbg!(Layout::new::<T>());
            let lay_f = dbg!(Layout::new::<F>());

            // let layout = lay_a;
            // let (layout, offset_f) = dbg!(layout.extend(lay_f).unwrap());

            unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(Self { f, t }))) }

            /* unsafe {
                let p = alloc(layout);
                if p.is_null() {
                    handle_alloc_error(layout);
                }
                *(&mut (*(p as *mut Self)).t) = t;
                *(&mut (*(p as *mut Self)).f) = f;
                /* (p as *mut T).write(t);
                (p.byte_add(offset_f) as *mut F).write(f);
                NonNull::new_unchecked(p as *mut Self) */
                NonNull::new_unchecked(p as *mut Self)
            } */
        }

        fn call(p: *const ()) {
            unsafe { ((*(p as *mut Self)).f)() };
        }
    }

    impl<T, F> Drop for Foo<T, F> {
        fn drop(&mut self) {
            println!("drop Foo");
        }
    }

    let foo;
    let raw;
    {
        let a = 23usize;
        let capture = 45i32;
        let f = move || println!("hello {capture}");
        foo = Foo::allocate(a, f);
        raw = foo.as_ptr() as *const ();
    }

    how_many();
    unsafe {
        // (&mut f as *mut dyn Fn()).drop_in_place();
        let foo = foo.as_ref();
        println!("{}", foo.t);
        (foo.f)();
    }

    unsafe {
        // let pa = Box::new(AllocateMe { a: 1, b: 2 });
        // how_many();
        // drop(pa);
        // how_many();

        // let expected = dbg!(std::mem::size_of::<AllocateMe>());
        // assert_eq!(expected, layout.size());
        // let pa = std::alloc::alloc(layout);
        // let pa = A.alloc(layout);
        // how_many();
        // let pb = std::alloc::alloc(layout);
        // how_many();
        //
        // std::alloc::dealloc(pa, layout);
        // how_many();
        // std::alloc::dealloc(pb, layout);
        // how_many();
    };
}
