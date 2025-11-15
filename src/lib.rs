pub mod executor;
pub mod kqueue;
pub mod parallel;
pub mod waker;

pub struct DropGuard<F: FnMut()>(pub F);

impl<F: FnMut()> Drop for DropGuard<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}
