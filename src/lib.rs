pub mod executor;
pub mod future;
pub mod parallel;
pub mod reactor;
pub mod task;

pub use executor::{Executor, sleep, spawn};

pub struct DropGuard<F: FnMut()>(pub F);

impl<F: FnMut()> Drop for DropGuard<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

pub fn setup_logging() {
    use tracing::level_filters::LevelFilter;
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::TRACE.into())
        .from_env()
        .unwrap();

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        // .with_thread_ids(true)
        .with_target(false)
        .without_time()
        .compact()
        .init();
}
