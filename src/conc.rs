use std::{future::Future, sync::Arc, time::Duration};
use tokio::{
    sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard},
    time,
};

use crate::error::Result;

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
    fn into_res(self) -> Result<T>;
}

impl<T> IntoResult<T> for Option<T> {
    fn into_res(self) -> Result<T> {
        self.ok_or("None value".into())
    }
}

impl<T, E> IntoResult<T> for std::result::Result<T, E>
where
    E: Into<Box<dyn std::error::Error>>,
{
    fn into_res(self) -> Result<T> {
        self.map_err(|e| e.into())
    }
}

pub async fn timeout<T, O, F>(secs: u64, f: F) -> Result<T>
where
    O: IntoResult<T>,
    F: Future<Output = O>,
{
    let res = time::timeout(Duration::from_secs(secs), f).await;
    match res {
        Ok(x) => x.into_res(),
        Err(_) => Err(format!("connectioned timed out after {secs}s").into()),
    }
}
