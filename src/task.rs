#![allow(unsafe_op_in_unsafe_fn)]

use std::{
    alloc::Layout,
    cell::UnsafeCell,
    mem::ManuallyDrop,
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};

use tracing::{debug, trace};

// Bit masks:
//
// ```txt
// [ usize_max..8 ]    [ bit 2 ]    [ bit 1 ]
//    ref_count         running     scheduled
// ```

/// Set if the task is scheduled.
///
/// A task is considered to be scheduled when a reference to it still exists
/// (has not been deallocated), therefore we can guarantee that constructing a
/// raw task from the opaque pointer will succeed.
pub(crate) const SCHEDULED: usize = 0;

/// Set if the task is **currently** being polled.
///
/// A task is in the running state _only_ when it is being polled.
pub(crate) const RUNNING: usize = 1 << 1;

/// Set if the task has completed.
///
/// A task is considered ready when polling its future returns `Poll::Ready`.
/// The output is then stored inside the [`RawTask`] until it becomes closed.
///
/// This flag **cannot** be set when the task is `scheduled` or `running`.
pub(crate) const READY: usize = 1 << 2;

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
        trace!("drop task");
    }
}

struct RawTaskVTable {
    schedule: unsafe fn(*const ()),
    poll: unsafe fn(*const ()),
    offsets: &'static RawTaskOffsets,
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

// For debugging purposes.
static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

// Const fns do not allow panics or capturing references so printing the actual
// `Display` of the error is not possible. This is a workaround.
macro_rules! unwrap {
    ($value:expr) => {{
        match $value {
            Ok(v) => v,
            Err(_) => panic!("called `Result::unwrap()` on an `Err` value"),
        }
    }};
}

impl<F, O, S> RawTask<F, O, S> {
    const LAYOUT: (Layout, RawTaskOffsets) = Self::layout();

    const fn layout() -> (Layout, RawTaskOffsets) {
        let lay_header = Layout::new::<Header>();
        let lay_f = Layout::new::<F>();
        let lay_o = Layout::new::<O>();
        let lay_s = Layout::new::<S>();

        let layout = lay_header;
        let (layout, future) = unwrap!(layout.extend(lay_f));
        let (layout, output) = unwrap!(layout.extend(lay_o));
        let (layout, schedule) = unwrap!(layout.extend(lay_s));

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

pub trait Scheduler {
    fn schedule(&self, task: Task);
}

impl<F: Fn(Task)> Scheduler for F {
    fn schedule(&self, task: Task) {
        self(task)
    }
}

impl<F, O, S> RawTask<F, O, S>
where
    F: Future<Output = O>,
    S: Scheduler,
{
    pub fn allocate(future: F, schedule: S) -> *const () {
        let (layout, offset) = Self::LAYOUT;
        unsafe {
            let p = std::alloc::alloc(layout) as *const ();
            let raw = Self::arrange(p);

            raw.header.write(Header {
                state: AtomicUsize::new(54),
                waker: UnsafeCell::new(None),
                vtable: &Self::VTABLE,
            });

            raw.future.write(future);

            raw.schedule.write(schedule);

            ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
            debug!(total = ?ALLOCATED, ?layout, ?offset, "allocate");

            p
        }
    }

    fn from_ptr(p: *const ()) -> Self {
        Self::arrange(p)
    }

    const VTABLE: RawTaskVTable = RawTaskVTable {
        schedule: schedule::<S>,
        poll: Self::poll,
        offsets: &Self::LAYOUT.1,
    };

    const RAW_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
        Self::clone_waker,
        Self::wake,
        Self::wake_by_ref,
        Self::drop_waker,
    );

    unsafe fn clone_waker(p: *const ()) -> RawWaker {
        trace!("waker clone");
        todo!()
    }

    unsafe fn wake(p: *const ()) {
        trace!("waker wake");
    }

    unsafe fn wake_by_ref(p: *const ()) {
        trace!("waker wake_by_ref");
        trace!("RawTask::schedule");
        // (*this.schedule)(task);
    }

    unsafe fn drop_waker(p: *const ()) {
        trace!("waker drop")
    }

    fn poll(p: *const ()) {
        trace!("polling!");
        let raw = Self::from_ptr(p);
        unsafe {
            let waker =
                ManuallyDrop::new(Waker::from_raw(RawWaker::new(p, &Self::RAW_WAKER_VTABLE)));
            let mut cx = Context::from_waker(&waker);

            match F::poll(Pin::new_unchecked(&mut *raw.future), &mut cx) {
                Poll::Ready(_) => {}
                Poll::Pending => {}
            }
        };
    }
}

unsafe fn schedule<S: Scheduler>(p: *const ()) {
    let header = &*(p as *const Header);
    let scheduler = p.byte_add(header.vtable.offsets.schedule) as *mut S;
    let task = Task { raw: p };
    (*scheduler).schedule(task);
    // header.vtable.offsets.schedule
    // let raw = Self::from_ptr(p);
    // let task = Task { raw: p };
    trace!("RawTask::schedule");
    // unsafe { (*raw.schedule)(task) };
}
