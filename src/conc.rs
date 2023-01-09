use anyhow;
use std::{future::Future, sync::Arc, time::Duration};
use tokio::{
    sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard},
    time,
};

#[derive(Debug)]
pub struct SharedMut<T> {
    inner: Arc<Mutex<T>>,
}

impl<T> Clone for SharedMut<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> SharedMut<T> {
    pub fn new(data: T) -> Self {
        Self {
            inner: Arc::new(Mutex::new(data)),
        }
    }

    pub async fn lock(&self) -> MutexGuard<'_, T> {
        self.inner.lock().await
    }
}

pub struct SharedRw<T> {
    inner: Arc<RwLock<T>>,
}

impl<T> Clone for SharedRw<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> SharedRw<T> {
    pub fn new(data: T) -> Self {
        Self {
            inner: Arc::new(RwLock::new(data)),
        }
    }

    pub async fn get(&self) -> RwLockReadGuard<'_, T> {
        self.inner.read().await
    }

    pub async fn get_mut(&self) -> RwLockWriteGuard<'_, T> {
        self.inner.write().await
    }
}

pub trait IntoResult<T> {
    fn into_res(self) -> anyhow::Result<T>;
}

impl<T> IntoResult<T> for Option<T> {
    fn into_res(self) -> anyhow::Result<T> {
        self.ok_or(anyhow::anyhow!("None value"))
    }
}

impl<T, E> IntoResult<T> for Result<T, E>
where
    E: Into<anyhow::Error>,
{
    fn into_res(self) -> anyhow::Result<T> {
        self.map_err(|e| e.into())
    }
}

pub async fn timeout<T, O, F>(secs: u64, f: F) -> anyhow::Result<T>
where
    O: IntoResult<T>,
    F: Future<Output = O>,
{
    let res = time::timeout(Duration::from_secs(secs), f).await;
    res.map(|o| o.into_res())
        .unwrap_or_else(|_| anyhow::bail!("connectioned timed out after {secs}s"))
}
