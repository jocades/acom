#![allow(unsafe_op_in_unsafe_fn)]

use std::{
    alloc::{Layout, alloc, dealloc, handle_alloc_error},
    cell::UnsafeCell,
    mem::ManuallyDrop,
    pin::Pin,
    process::abort,
    sync::atomic::{
        AtomicUsize,
        Ordering::{AcqRel, Acquire, Relaxed, Release},
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
pub(crate) const SCHEDULED: usize = 1;

/// Set if the task is **currently** being polled.
///
/// A task is in the running state **only** when it is being polled.
pub(crate) const RUNNING: usize = 1 << 1;

/// Set if the task has completed.
///
/// A task is considered ready when polling its future returns `Poll::Ready`.
/// The output is then stored inside the [`RawTask`] until it becomes closed.
/// When the [`JoinHandle`] retrieves the output it marks the task `closed`.
///
/// This flag **cannot** be set when the task is `scheduled` or `running`.
pub(crate) const READY: usize = 1 << 2;

/// Set if the task is closed.
///
/// If a task is closed, that means it's either canceled or its output has been consumed by the
/// `JoinHandle`. A task becomes closed when:
///
/// 1. It gets canceled by `Task::cancel()`, `Task::drop()`, or `JoinHandle::cancel()`.
/// 2. Its output gets awaited by the `JoinHandle`.
/// 3. It panics while polling the future.
/// 4. It is completed and the `JoinHandle` gets dropped.
pub(crate) const CLOSED: usize = 1 << 3;

/// Set if the `JoinHandle` still exists.
///
/// The `JoinHandle` is a special case in that it is only tracked by this flag, while all other
/// task references (`Task` and `Waker`s) are tracked by the reference count.
pub(crate) const HANDLE: usize = 1 << 4;

/// Set if the `JoinHandle` is awaiting the output.
///
/// This flag is set while there is a registered awaiter of type `Waker` inside the task. When the
/// task gets closed or completed, we need to wake the awaiter. This flag can be used as a fast
/// check that tells us if we need to wake anyone.
pub(crate) const AWAITER: usize = 1 << 5;

/// The padding (in bits) to the first bit of the ref count.
pub(crate) const REF_PAD: usize = 8;

/// The top bits reserved for counting how many wakers have been cloned.
pub(crate) const REF_COUNT: usize = 1 << REF_PAD;

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
    deallocate: unsafe fn(*const ()),
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
    scheduler: *mut S,
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
    scheduler: usize,
}

// For debugging purposes.
static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

impl<F, O, S> RawTask<F, O, S> {
    const LAYOUT: (Layout, RawTaskOffsets) = Self::layout();

    const fn layout() -> (Layout, RawTaskOffsets) {
        // Const fns do not allow calling non constant functions.
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
        let lay_scheduler = Layout::new::<S>();

        let layout = lay_header;
        let (layout, future) = unwrap!(layout.extend(lay_future));
        let (layout, output) = unwrap!(layout.extend(lay_output));
        let (layout, scheduler) = unwrap!(layout.extend(lay_scheduler));

        (
            layout,
            RawTaskOffsets {
                future,
                output,
                scheduler,
            },
        )
    }

    fn arrange(p: *const ()) -> Self {
        unsafe {
            Self {
                header: p as *mut Header,
                future: p.byte_add(Self::LAYOUT.1.future) as *mut F,
                output: p.byte_add(Self::LAYOUT.1.output) as *mut O,
                scheduler: p.byte_add(Self::LAYOUT.1.scheduler) as *mut S,
            }
        }
    }
}

macro_rules! scheduler {
    ($p:expr) => {
        (*($p.byte_add(Self::LAYOUT.1.scheduler) as *mut S))
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
        deallocate: Self::deallocate,
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

            this.scheduler.write(schedule);

            ALLOCATED.fetch_add(layout.size(), Relaxed);
            debug!(?ALLOCATED, ?layout, ?offset, "allocate");

            p
        }
    }

    unsafe fn deallocate(p: *const ()) {
        // Drop the schedule callback and what it may have captured.
        let scheduler = p.byte_add(Self::LAYOUT.1.scheduler) as *mut S;
        scheduler.drop_in_place();

        // Drop the task, the future allocated via `Box` will be deallocated
        // in the executor when the task gets dropped.
        dealloc(p as *mut u8, Self::LAYOUT.0);
        ALLOCATED.fetch_sub(Self::LAYOUT.0.size(), Relaxed);
        debug!(?p, ?ALLOCATED, "deallocate");
    }

    unsafe fn schedule(p: *const ()) {
        trace!("schedule");
        scheduler!(p).schedule(Task { raw: p });
    }

    unsafe fn clone_waker(p: *const ()) -> RawWaker {
        trace!("waker clone");
        let state = header!(p).state.fetch_add(REF_COUNT, Relaxed);
        dbg_state(state + REF_COUNT);
        assert!(state < isize::MAX as usize);
        RawWaker::new(p, &Self::RAW_WAKER_VTABLE)
    }

    /// Drops a waker.
    ///
    /// This function will decrement the reference count. If it drops down to zero, the associated
    /// join handle has been dropped too, and the task has not been completed, then it will get
    /// scheduled one more time so that its future gets dropped by the executor.
    unsafe fn drop_waker(p: *const ()) {
        debug!("drop_waker");
        let header = &header!(p);
        let state = header.state.fetch_sub(REF_COUNT, AcqRel) - REF_COUNT;

        dbg_state(state);

        if (state >> REF_PAD) == 0 {
            Self::deallocate(p);
        } else {
            // Self::schedule(p);
        }
    }

    unsafe fn wake(p: *const ()) {
        trace!(?p, "wake");

        let header = &header!(p);
        let mut state = header.state.load(Acquire);

        loop {
            let new = state | SCHEDULED;
            match header
                .state
                .compare_exchange_weak(state, new, AcqRel, Acquire)
            {
                Ok(_) => {
                    if state & RUNNING == 0 {
                        Self::schedule(p);
                    } else {
                        Self::drop_waker(p);
                    }
                    break;
                }
                Err(act) => state = act,
            }
        }
    }

    unsafe fn wake_by_ref(p: *const ()) {
        trace!(?p, "wake_by_ref");

        let header = &*(p as *const Header);
        let mut state = header.state.load(Acquire);

        loop {
            // If the task is not running, we can schedule right away.
            let new = if state & RUNNING == 0 {
                (state | SCHEDULED) + REF_COUNT
            } else {
                state | SCHEDULED
            };

            match header
                .state
                .compare_exchange_weak(state, new, AcqRel, Acquire)
            {
                Ok(_) => {
                    if state & RUNNING == 0 {
                        assert!(state < isize::MAX as usize);
                        Self::schedule(p);
                    }
                    break;
                }
                Err(act) => state = act,
            }
        }
    }

    unsafe fn drop_task(p: *const ()) {
        let state = header!(p).state.fetch_sub(REF_COUNT, AcqRel) - REF_COUNT;
        if (state >> REF_PAD) == 0 && state & HANDLE == 0 {
            Self::deallocate(p);
        }
    }

    unsafe fn drop_future(p: *const ()) {
        (p.byte_add(Self::LAYOUT.1.future) as *mut F).drop_in_place();
    }

    fn poll(p: *const ()) {
        trace!(?p, "poll");
        unsafe {
            let header = &header!(p);

            let waker = (*header.waker.get())
                .get_or_insert(Waker::from_raw(RawWaker::new(p, &Self::RAW_WAKER_VTABLE)));
            let mut cx = Context::from_waker(&waker);

            let mut state = header.state.load(Acquire);

            loop {
                // Mark the state as unscheduled and running.
                let new = (state & !SCHEDULED) | RUNNING;
                match header
                    .state
                    .compare_exchange_weak(state, new, AcqRel, Acquire)
                {
                    Ok(_) => {
                        state = new;
                        break;
                    }
                    Err(act) => state = act,
                };
            }

            match F::poll(Pin::new_unchecked(future_mut!(p)), &mut cx) {
                Poll::Ready(_) => {
                    trace!(?p, "ready");
                    // Mark the task as ready and not running nor scheduled.
                    loop {
                        let new = (state & !SCHEDULED & !RUNNING) | READY;
                        match header
                            .state
                            .compare_exchange_weak(state, new, AcqRel, Acquire)
                        {
                            Ok(_) => {
                                // Drop the task.
                                Self::drop_task(p);
                                break;
                            }
                            Err(act) => state = act,
                        }
                    }
                }
                Poll::Pending => {
                    trace!(?p, "pending");
                    // Mark the task as not running.
                    loop {
                        let new = state & !RUNNING;
                        match header
                            .state
                            .compare_exchange_weak(state, new, AcqRel, Acquire)
                        {
                            Ok(_) => {
                                break;
                            }
                            Err(act) => state = act,
                        }
                    }
                }
            }
        };
    }
}

fn dbg_state(s: usize) {
    println!(
        "scheduled = {} ready = {} ref_count = {}",
        s & SCHEDULED != 0,
        s & READY != 0,
        s >> REF_PAD
    );
}
