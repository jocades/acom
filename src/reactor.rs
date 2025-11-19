use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::ptr::{null, null_mut};
use std::sync::OnceLock;
use std::task::Waker;
use std::time::Duration;

use libc as c;

macro_rules! sc {
    ($fn:ident$args:tt) => {{
        #[allow(unused_unsafe)] // Discard warnings on nested unsafe blocks.
        let r = unsafe { libc::$fn$args };
        if r < 0 { Err(std::io::Error::last_os_error()) } else { Ok(r) }
    }};
}

macro_rules! kevent {
    () => {
        unsafe { std::mem::zeroed::<libc::kevent>() }
    };

    ($id:expr, $filter:expr, $flags:expr) => {{
        libc::kevent {
            ident: $id as usize,
            filter: $filter as i16,
            flags: $flags,
            fflags: 0,
            data: 0,
            udata: std::ptr::null_mut(),
        }
    }};
}

pub struct Reactor {
    kq: OwnedFd,
}

/// A wrapper around a [kqueue](https://man.freebsd.org/cgi/man.cgi?kqueue(2))
/// file descriptor.
impl Reactor {
    pub const MAX_EVENTS: usize = 64;

    pub fn get() -> &'static Self {
        static REACTOR: OnceLock<Reactor> = OnceLock::new();
        REACTOR.get_or_init(|| Reactor::new().expect("open kqueue"))
    }

    pub fn new() -> io::Result<Self> {
        let kq = unsafe { OwnedFd::from_raw_fd(sc!(kqueue())?) };
        Ok(Self { kq })
    }

    fn register(&self, kev: &c::kevent) -> io::Result<()> {
        unsafe { c::kevent(self.kq.as_raw_fd(), kev, 1, null_mut(), 0, null()) };
        Ok(())
    }

    pub fn wake_on_timer(&self, dur: Duration, waker: Waker) -> io::Result<()> {
        let boxed = Box::into_raw(Box::new(waker)) as *mut c::c_void;
        let mut kev = kevent!(boxed as usize, c::EVFILT_TIMER, c::EV_ADD | c::EV_ONESHOT);
        kev.data = dur.as_millis() as isize;
        kev.udata = boxed;
        self.register(&kev)
    }

    pub fn wait(&self, events: &mut [c::kevent]) -> usize {
        let nev = unsafe {
            c::kevent(
                self.kq.as_raw_fd(),
                null(),
                0,
                events.as_mut_ptr(),
                events.len() as i32,
                null(),
            )
        } as usize;
        return nev;
    }
}
