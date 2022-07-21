use std::io::{Error as IOError, ErrorKind as IOErrorKind};
use std::os::unix::io::AsRawFd;

use nix::{
    errno::Errno,
    fcntl::{flock, FlockArg},
    Error as NixError,
};

pub enum LockType {
    Exclusive,
    Shared,
}

#[derive(Debug)]
pub enum Error {
    InvalidFd,
    Interrupted,
    InvalidOperation,
    OutOfMemory,
    WouldBlock,
    Other(NixError),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use Error::*;
        match self {
            InvalidFd => write!(f, "The provided item is not an open file descriptor."),
            Interrupted => write!(f, "While waiting to acquire a lock, the call was interrupted by delivery of a signal caught by a handler."),
            InvalidOperation => write!(f, "File locking operation is invalid."),
            OutOfMemory => write!(f, "The kernel ran out of memory for allocating lock records."),
            WouldBlock => write!(f, "The file is locked and the blocking flag was set to false."),
            Other(e) => write!(f, "Non-flock error: {}", e),
        }
    }
}
impl std::error::Error for Error {}
impl From<NixError> for Error {
    fn from(e: NixError) -> Self {
        match e {
            Errno::EBADF => Error::InvalidFd,
            Errno::EINTR => Error::Interrupted,
            Errno::EINVAL => Error::InvalidOperation,
            Errno::ENOLCK => Error::OutOfMemory,
            Errno::EWOULDBLOCK => Error::WouldBlock,
            _ => Error::Other(e),
        }
    }
}
impl From<Error> for IOError {
    fn from(e: Error) -> IOError {
        use Error::*;
        match e {
            InvalidFd | InvalidOperation => IOError::new(IOErrorKind::InvalidInput, e),
            Interrupted => IOError::new(IOErrorKind::Interrupted, e),
            OutOfMemory | Other(_) => IOError::new(IOErrorKind::Other, e),
            WouldBlock => IOError::new(IOErrorKind::WouldBlock, e),
        }
    }
}

pub struct FdLock<F: AsRawFd>(Option<F>);
impl<F: AsRawFd> std::ops::Deref for FdLock<F> {
    type Target = F;
    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}
impl<F: AsRawFd> std::ops::DerefMut for FdLock<F> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut().unwrap()
    }
}
impl<F: AsRawFd> FdLock<F> {
    pub fn lock(f: F, lock_type: LockType, blocking: bool) -> Result<Self, Error> {
        flock(
            f.as_raw_fd(),
            match lock_type {
                LockType::Exclusive => {
                    if blocking {
                        FlockArg::LockExclusive
                    } else {
                        FlockArg::LockExclusiveNonblock
                    }
                }
                LockType::Shared => {
                    if blocking {
                        FlockArg::LockShared
                    } else {
                        FlockArg::LockSharedNonblock
                    }
                }
            },
        )?;
        Ok(FdLock(Some(f)))
    }
    pub fn map<Func: FnOnce(F) -> F_, F_: AsRawFd>(mut self, map_fn: Func) -> FdLock<F_> {
        FdLock(self.0.take().map(map_fn))
    }
    pub fn unlock(mut self, blocking: bool) -> Result<F, (Self, Error)> {
        match flock(
            self.0.as_ref().unwrap().as_raw_fd(),
            if blocking {
                FlockArg::Unlock
            } else {
                FlockArg::UnlockNonblock
            },
        ) {
            Ok(()) => Ok(self.0.take().unwrap()),
            Err(e) => Err((self, e.into())),
        }
    }
}
impl<F: AsRawFd> std::ops::Drop for FdLock<F> {
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            flock(f.as_raw_fd(), FlockArg::Unlock).unwrap()
        }
    }
}
