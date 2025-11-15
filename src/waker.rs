/// A module providing utilities to create Waker objects from Arc-wrapped types.
/// This allows tasks to be woken up by the reactor when events occur.
use std::{
    mem::ManuallyDrop,
    sync::Arc,
    task::{RawWaker, RawWakerVTable, Waker},
};

const fn vtable<T: ArcWake + 'static>() -> &'static RawWakerVTable {
    &RawWakerVTable::new(clone::<T>, wake::<T>, wake_by_ref::<T>, drop::<T>)
}

/// Create a Waker from an Arc-wrapped type implementing ArcWake.
pub fn waker<T: ArcWake + 'static>(wake: Arc<T>) -> Waker {
    let p = Arc::into_raw(wake) as *const ();
    unsafe { Waker::from_raw(RawWaker::new(p, vtable::<T>())) }
}

fn clone<T: ArcWake + 'static>(p: *const ()) -> RawWaker {
    // Retain Arc, but don't touch refcount by wrapping in ManuallyDrop
    let arc = ManuallyDrop::new(unsafe { Arc::from_raw(p.cast::<T>()) });
    // Now increase refcount, but don't drop new refcount either
    let _ = arc.clone();
    RawWaker::new(p, vtable::<T>())
}

fn wake<T: ArcWake + 'static>(p: *const ()) {
    let arc = unsafe { Arc::from_raw(p.cast::<T>()) };
    ArcWake::wake(arc);
}

fn wake_by_ref<T: ArcWake + 'static>(p: *const ()) {
    // Retain Arc, but don't touch refcount by wrapping in ManuallyDrop
    let arc = ManuallyDrop::new(unsafe { Arc::from_raw(p.cast::<T>()) });
    ArcWake::wake_by_ref(&arc);
}

fn drop<T: ArcWake + 'static>(p: *const ()) {
    std::mem::drop(unsafe { Arc::from_raw(p.cast::<T>()) });
}

/// A way of waking up a specific task.
///
/// By implementing this trait, types that are expected to be wrapped in an `Arc`
/// can be converted into [`Waker`] objects.
/// Those Wakers can be used to signal executors that a task it owns
/// is ready to be `poll`ed again.
pub trait ArcWake: Send + Sync {
    /// Indicates that the associated task is ready to make progress and should
    /// be `poll`ed.
    ///
    /// This function can be called from an arbitrary thread, including threads which
    /// did not create the `ArcWake` based [`Waker`].
    ///
    /// Executors generally maintain a queue of "ready" tasks; `wake` should place
    /// the associated task onto this queue.
    ///
    /// [`Waker`]: std::task::Waker
    fn wake(self: Arc<Self>) {
        Self::wake_by_ref(&self)
    }

    /// Indicates that the associated task is ready to make progress and should
    /// be `poll`ed.
    ///
    /// This function can be called from an arbitrary thread, including threads which
    /// did not create the `ArcWake` based [`Waker`].
    ///
    /// Executors generally maintain a queue of "ready" tasks; `wake_by_ref` should place
    /// the associated task onto this queue.
    ///
    /// This function is similar to [`wake`](ArcWake::wake), but must not consume the provided data
    /// pointer.
    ///
    /// [`Waker`]: std::task::Waker
    fn wake_by_ref(arc_self: &Arc<Self>);
}
