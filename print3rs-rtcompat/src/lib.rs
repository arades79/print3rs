//use futures_lite::prelude::*;

use core::{fmt::Debug, future::Future};

#[cfg(feature = "tokio")]
pub use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite,
    AsyncWriteExt, BufReader, BufStream,
};

#[cfg(feature = "smol")]
pub use smol::{
    AsyncBufRead,
    AsyncBufReadExt,
    AsyncRead,
    AsyncReadExt,
    AsyncWrite,
    AsyncWriteExt, // TODO: finish up traits for parity
};

#[cfg(feature = "time")]
pub mod time;

#[cfg(feature = "fs")]
pub mod fs;

#[cfg(feature = "sync")]
pub mod sync;

pub trait BackgroundFuture: Future + Debug {
    fn cancel(self);
    fn detach(self);
}

#[cfg(feature = "tokio")]
impl<T: Debug> BackgroundFuture for tokio::task::JoinHandle<T> {
    fn cancel(self) {
        self.abort()
    }
    fn detach(self) {}
}

#[cfg(feature = "smol")]
impl<T> BackgroundFuture for smol::Task<T> {
    fn cancel(self) {
        self.cancel()
    }
    fn detach(self) {
        self.detach()
    }
}

#[cfg(feature = "tokio")]
pub type Task<T> = tokio::task::JoinHandle<T>;

pub fn spawn<F: Future + Send + 'static>(fut: F) -> Task<F::Output>
where
    F::Output: Send + 'static + Debug,
    Task<F::Output>: BackgroundFuture,
{
    #[cfg(feature = "tokio")]
    tokio::spawn(fut)
}
