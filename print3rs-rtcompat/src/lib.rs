//use futures_lite::prelude::*;

use {core::fmt::Debug, futures_lite::Future};

#[cfg(feature = "tokio")]
pub use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite,
    AsyncWriteExt, BufReader, BufStream,
};

#[cfg(not(feature = "tokio"))]
pub use futures_lite::{
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

#[cfg(all(not(feature = "tokio"), feature = "fs"))]
pub use async_io::*;

#[cfg(all(feature = "tokio", feature = "net"))]
pub use tokio::time;

#[cfg(all(not(feature = "tokio"), feature = "net"))]
pub use async_io::*;

pub trait BackgroundFuture: Future + Debug {
    fn cancel(self);
    fn detach(self);
}

pub trait Spawner {
    type Transform<T>: Debug
    where
        T: Debug;
    fn spawn<F: Future + Send + 'static>(
        &self,
        fut: F,
    ) -> impl BackgroundFuture<Output = Self::Transform<F::Output>>
    where
        F::Output: Send + 'static + Debug;
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

pub type Task<T> = tokio::task::JoinHandle<T>;

pub fn spawn<F: Future + Send + 'static>(fut: F) -> Task<F::Output>
where
    F::Output: Send + 'static + Debug,
    Task<F::Output>: BackgroundFuture,
{
    #[cfg(feature = "tokio")]
    {
        tokio::spawn(fut)
    }
}
