#![allow(unsafe_op_in_unsafe_fn)]

use std::{
    alloc::{Layout, alloc, dealloc, handle_alloc_error},
    cell::UnsafeCell,
    pin::Pin,
    process::abort,
    sync::atomic::{
        AtomicUsize,
        Ordering::{Acquire, Relaxed, Release},
    },
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};

use tracing::{debug, instrument, trace};

// Bit masks:
//
// ```txt
// [ usize_max..=8 ]    [ 8..2 ]    [ bit 2 ]    [ bit 1 ]
//     ref_count        reserved     running     scheduled
// ```

/// Set if the task is scheduled.
///
/// A task is considered to be scheduled when a reference to it still exists
/// (has not been deallocated), therefore we can guarantee that constructing a
/// raw task from the opaque pointer will succeed.
pub(crate) const SCHEDULED: usize = 0;

/// Set if the task is **currently** being polled.
///
/// A task is in the running state **only** when it is being polled.
pub(crate) const RUNNING: usize = 1 << 1;

/// Set if the task has completed.
///
/// A task is considered ready when polling its future returns `Poll::Ready`.
/// The output is then stored inside the [`RawTask`] until it becomes closed.
///
/// This flag **cannot** be set when the task is `scheduled` or `running`.
pub(crate) const READY: usize = 1 << 2;

/// The top bits reserved for counting how many wakers have been cloned.
pub(crate) const REF_COUNT: usize = 1 << 8;

#[derive(Debug)]
pub struct Task {
    /// A pointer to a heap allocated [RawTask].
    pub(crate) raw: *const (),
}

macro_rules! forget {
    ($task:expr) => {{
        let p = $task.raw;
        std::mem::forget($task);
        p
    }};
}

macro_rules! header {
    ($raw:expr) => {
        (*($raw as *const Header))
    };
}

impl Task {
    pub fn schedule(self) {
        let p = forget!(self);
        unsafe { (header!(p).vtable.schedule)(p) };
    }

    pub fn poll(self) {
        let p = forget!(self);
        unsafe { (header!(p).vtable.poll)(p) };
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        debug!("drop task");
    }
}

pub trait Scheduler {
    fn schedule(&self, task: Task);
}

impl<F: Fn(Task)> Scheduler for F {
    fn schedule(&self, task: Task) {
        self(task)
    }
}

struct RawTaskVTable {
    schedule: unsafe fn(*const ()),
    poll: unsafe fn(*const ()),
}

struct Header {
    state: AtomicUsize,
    waker: UnsafeCell<Option<Waker>>,
    vtable: &'static RawTaskVTable,
}

pub struct RawTask<F, O, S> {
    header: *mut Header,
    future: *mut F,
    output: *mut O,
    schedule: *mut S,
}

impl<F, O, S> Copy for RawTask<F, O, S> {}
impl<F, O, S> Clone for RawTask<F, O, S> {
    fn clone(&self) -> Self {
        *self
    }
}

#[derive(Debug)]
struct RawTaskOffsets {
    future: usize,
    output: usize,
    schedule: usize,
}

struct RawTaskLayout {
    layout: Layout,
    offset: RawTaskOffsets,
}

// For debugging purposes.
static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

impl<F, O, S> RawTask<F, O, S> {
    const LAYOUT: (Layout, RawTaskOffsets) = Self::layout();

    const fn layout() -> (Layout, RawTaskOffsets) {
        // Const fns do not allow calling non constant functions (`.unwrap()`)
        // nor capturing references so displaying the error is not possible.
        macro_rules! unwrap {
            ($value:expr) => {{
                match $value {
                    Ok(v) => v,
                    Err(_) => panic!("called `Result::unwrap()` on an `Err` value"),
                }
            }};
        }

        let lay_header = Layout::new::<Header>();
        let lay_future = Layout::new::<F>();
        let lay_output = Layout::new::<O>();
        let lay_schedule = Layout::new::<S>();

        let layout = lay_header;
        let (layout, future) = unwrap!(layout.extend(lay_future));
        let (layout, output) = unwrap!(layout.extend(lay_output));
        let (layout, schedule) = unwrap!(layout.extend(lay_schedule));

        (
            layout,
            RawTaskOffsets {
                future,
                output,
                schedule,
            },
        )
    }

    fn arrange(p: *const ()) -> Self {
        unsafe {
            Self {
                header: p as *mut Header,
                future: p.byte_add(Self::LAYOUT.1.future) as *mut F,
                output: p.byte_add(Self::LAYOUT.1.output) as *mut O,
                schedule: p.byte_add(Self::LAYOUT.1.schedule) as *mut S,
            }
        }
    }
}

macro_rules! scheduler {
    ($p:expr) => {
        (*($p.byte_add(Self::LAYOUT.1.schedule) as *mut S))
    };
}

macro_rules! future_mut {
    ($p:expr) => {
        (&mut *($p.byte_add(Self::LAYOUT.1.future) as *mut F))
    };
}

impl<F, O, S> RawTask<F, O, S>
where
    F: Future<Output = O>,
    S: Scheduler,
{
    const VTABLE: RawTaskVTable = RawTaskVTable {
        schedule: Self::schedule,
        poll: Self::poll,
    };

    const RAW_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
        Self::clone_waker,
        Self::wake,
        Self::wake_by_ref,
        Self::drop_waker,
    );

    fn from_ptr(p: *const ()) -> Self {
        Self::arrange(p)
    }

    pub fn allocate(future: F, schedule: S) -> *const () {
        let (layout, offset) = Self::LAYOUT;
        unsafe {
            let p = alloc(layout) as *const ();
            if p.is_null() {
                handle_alloc_error(layout);
            }

            let this = Self::arrange(p);

            this.header.write(Header {
                state: AtomicUsize::new(SCHEDULED | REF_COUNT),
                waker: UnsafeCell::new(None),
                vtable: &Self::VTABLE,
            });

            this.future.write(future);

            this.schedule.write(schedule);

            ALLOCATED.fetch_add(layout.size(), Relaxed);
            debug!(total = ?ALLOCATED, ?layout, ?offset, "allocate");

            p
        }
    }

    unsafe fn deallocate(p: *const ()) {
        // Drop the schedule callback and what it may have captured.
        let schedule = p.byte_add(Self::LAYOUT.1.schedule) as *mut S;
        schedule.drop_in_place();

        // Drop the task, the future allocated via `Box` will be deallocated
        // in the executor when the task gets dropped.
        dealloc(p as *mut u8, Self::LAYOUT.0);
        ALLOCATED.fetch_sub(Self::LAYOUT.0.size(), Relaxed);
    }

    unsafe fn schedule(p: *const ()) {
        trace!("schedule");
        scheduler!(p).schedule(Task { raw: p });
        // let scheduler = p.byte_add(Self::LAYOUT.1.schedule) as *mut S;
        // (*(p.byte_add(Self::LAYOUT.1.schedule) as *mut S)).schedule(Task { raw: p });
        // (*scheduler).schedule(Task { raw: p });
    }

    fn poll(p: *const ()) {
        unsafe {
            let waker = (*header!(p).waker.get())
                .get_or_insert_with(|| Waker::from_raw(RawWaker::new(p, &Self::RAW_WAKER_VTABLE)));
            let mut cx = Context::from_waker(&waker);

            match F::poll(Pin::new_unchecked(future_mut!(p)), &mut cx) {
                Poll::Ready(_) => {
                    trace!("ready")
                }
                Poll::Pending => {
                    trace!("pending")
                }
            }
        };
    }

    unsafe fn clone_waker(p: *const ()) -> RawWaker {
        trace!("waker clone");
        let state = header!(p).state.fetch_add(REF_COUNT, Relaxed);

        if state > isize::MAX as usize {
            panic!("state overflow");
        }

        RawWaker::new(p, &Self::RAW_WAKER_VTABLE)
    }

    unsafe fn wake(p: *const ()) {
        let state = &header!(p).state.load(Acquire);
    }

    unsafe fn wake_by_ref(p: *const ()) {
        trace!("waker wake_by_ref");
        scheduler!(p).schedule(Task { raw: p });
    }

    /// Drops a waker.
    ///
    /// This function will decrement the reference count. If it drops down to zero, the associated
    /// join handle has been dropped too, and the task has not been completed, then it will get
    /// scheduled one more time so that its future gets dropped by the executor.
    unsafe fn drop_waker(p: *const ()) {}
}
