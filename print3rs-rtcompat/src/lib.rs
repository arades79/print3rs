//use futures_lite::prelude::*;

use {
    core::fmt::Debug,
    futures_lite::{Future, FutureExt},
};
pub trait Task: Future + Debug {
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
    ) -> impl Task<Output = Self::Transform<F::Output>>
    where
        F::Output: Send + 'static + Debug;
}

#[cfg(feature = "tokio")]
#[derive(Debug)]
pub struct AutoCloseJoinHandle<T>(tokio::task::JoinHandle<T>);

#[cfg(feature = "tokio")]
impl<T> Drop for AutoCloseJoinHandle<T> {
    fn drop(&mut self) {
        self.0.abort()
    }
}

#[cfg(feature = "tokio")]
impl<T> Future for AutoCloseJoinHandle<T> {
    type Output = <tokio::task::JoinHandle<T> as Future>::Output;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.0.poll(cx)
    }
}

#[cfg(feature = "tokio")]
impl<T: Debug> Task for AutoCloseJoinHandle<T> {
    fn cancel(self) {
        self.0.abort()
    }
    fn detach(self) {}
}

#[cfg(feature = "smol")]
impl<T> Task for smol::Task<T> {
    fn cancel(self) {
        self.cancel()
    }
    fn detach(self) {
        self.detach()
    }
}

#[cfg(feature = "tokio")]
impl Spawner for tokio::runtime::Handle {
    type Transform<T> = Result<T, tokio::task::JoinError> where T: Debug;
    fn spawn<F: Future + Send + 'static>(
        &self,
        fut: F,
    ) -> impl Task<Output = Self::Transform<F::Output>>
    where
        F::Output: Send + 'static + Debug,
    {
        AutoCloseJoinHandle(self.spawn(fut))
    }
}

#[cfg(feature = "smol")]
impl Spawner for smol::Executor {
    type Transform<T> = T;
    fn spawn<F: Future + Send + 'static>(
        &self,
        fut: F,
    ) -> impl Task<Output = Self::Transform<F::Output>>
    where
        F::Output: Send + 'static,
    {
        self.spawn(fut)
    }
}

#[cfg(feature = "tokio")]
type GlobalSpawner = tokio::runtime::Handle;

#[cfg(all(feature = "smol", not(feature = "tokio")))]
type GlobalSpawner = smol::Executor;

pub fn spawn<F: Future + Send + 'static>(
    fut: F,
) -> impl Task<Output = <GlobalSpawner as Spawner>::Transform<F::Output>>
where
    GlobalSpawner: Spawner,
    F::Output: Send + 'static + Debug,
{
    #[cfg(feature = "tokio")]
    {
        static SPAWNER: std::sync::OnceLock<tokio::runtime::Handle> = std::sync::OnceLock::new();
        let spawner = SPAWNER.get_or_init(tokio::runtime::Handle::current);
        Spawner::spawn(spawner, fut)
    }
}

pub type BackgroundTask<T> = dyn Task<Output = <GlobalSpawner as Spawner>::Transform<T>>;
