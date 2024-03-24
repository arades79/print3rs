#[cfg(feature = "tokio")]
use tokio::time;

#[cfg(feature = "smol")]
use async_io::*;

use futures_lite::Future;

pub struct TimeoutError;

pub async fn timeout<F>(dur: std::time::Duration, fut: F) -> Option<F::Output>
where
    F: Future,
{
    #[cfg(feature = "tokio")]
    {
        let expected = time::timeout(dur, fut).await.ok()?;
        Some(expected)
    }
    #[cfg(not(feature = "tokio"))]
    {
        todo!()
    }
}
