use std::{future::Future, sync::Arc, time::Duration};
use tokio::{
    sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard},
    time,
};

use crate::error::Result;

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

pub async fn timeout<F, T>(secs: u64, f: F) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    time::timeout(Duration::from_secs(secs), f)
        .await
        .unwrap_or(Err(format!("timed out after {secs}s").into()))
}
