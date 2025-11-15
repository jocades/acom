#![allow(unused_unsafe)]

use libc as c;
use std::{
    io,
    os::fd::{FromRawFd, OwnedFd, RawFd},
};

macro_rules! sc {
    ($fn:ident$args:tt) => {{
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

struct KQueue {
    kq: OwnedFd,
}

/// A wrapper around a [kqueue](https://man.freebsd.org/cgi/man.cgi?kqueue(2))
/// file descriptor.
impl KQueue {
    fn new() -> io::Result<Self> {
        let kq = unsafe { OwnedFd::from_raw_fd(sc!(kqueue())?) };
        Ok(Self { kq })
    }
}
