use std::sync::Arc;
use tokio::sync::{Mutex, MutexGuard};

pub struct SharedRef<T> {
    inner: Arc<Mutex<T>>,
}

impl<T> Clone for SharedRef<T> {
    fn clone(self: &Self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> SharedRef<T> {
    pub fn new(data: T) -> Self {
        Self {
            inner: Arc::new(Mutex::new(data)),
        }
    }

    pub async fn get_handle<'a>(self: &'a Self) -> MutexGuard<'a, T> {
        self.inner.lock().await
    }
}
