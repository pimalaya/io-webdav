//! Generator-shape coroutine driver. Mirrors `core::ops::Coroutine`: a `Yield`
//! associated type for intermediate progress, a `Return` for terminal output,
//! and a two-variant [`WebdavCoroutineState`].
//!
//! Most coroutines pick the standard [`WebdavYield`] (I/O-only); redirect-aware
//! ones declare their own (e.g. [`WebdavRedirectYield`]).
//!
//! [`WebdavRedirectYield`]: crate::rfc4918::coroutine::WebdavRedirectYield

use alloc::vec::Vec;

/// State yielded by a [`WebdavCoroutine::resume`] step.
#[derive(Debug)]
pub enum WebdavCoroutineState<Y, R> {
    /// Intermediate yield: the driver reacts and resumes.
    Yielded(Y),
    /// Terminal yield. By convention `R = Result<Output, Error>`.
    Complete(R),
}

/// Standard-shape WebDAV coroutine.
pub trait WebdavCoroutine {
    /// Intermediate value handed back on every step.
    type Yield;
    /// Terminal value. By convention `Result<Output, Error>`.
    type Return;

    /// Advances the coroutine one step.
    ///
    /// Pass [`None`] on the initial call or after a
    /// [`WebdavYield::WantsWrite`]. Pass `Some(data)` after a
    /// [`WebdavYield::WantsRead`]; `Some(&[])` signals EOF.
    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return>;
}

/// Standard I/O-only Yield for coroutines that only read/write socket bytes.
#[derive(Debug)]
pub enum WebdavYield {
    /// Driver should read more bytes and feed them back on the next resume.
    WantsRead,
    /// Driver should write these bytes; the next resume typically takes `None`.
    WantsWrite(Vec<u8>),
}

/// Coroutine `?`: forwards `Yielded` (via `Into`), short-circuits on `Err` (via
/// `Into`), evaluates to the inner `Ok` value.
#[macro_export]
macro_rules! webdav_try {
    ($coroutine:expr, $arg:expr $(,)?) => {
        match $crate::coroutine::WebdavCoroutine::resume($coroutine, $arg) {
            $crate::coroutine::WebdavCoroutineState::Yielded(y) => {
                return $crate::coroutine::WebdavCoroutineState::Yielded(y.into());
            }
            $crate::coroutine::WebdavCoroutineState::Complete(Err(err)) => {
                return $crate::coroutine::WebdavCoroutineState::Complete(Err(err.into()));
            }
            $crate::coroutine::WebdavCoroutineState::Complete(Ok(value)) => value,
        }
    };
}
