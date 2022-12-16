use std::sync::Arc;
use tokio::sync::{Mutex, MutexGuard};

pub struct SharedRef<T> {
    inner: Option<Arc<Mutex<T>>>,
}

impl<T> Clone for SharedRef<T> {
    fn clone(self: &Self) -> Self {
        Self {
            inner: self.inner.as_ref().map(|mapref| Arc::clone(&mapref)),
        }
    }
}

impl<T> SharedRef<T> {
    pub fn new(data: Option<T>) -> Self {
        Self {
            inner: data.map(|mapref| Arc::new(Mutex::new(mapref))),
        }
    }
    pub async fn get_handle<'a>(self: &'a Self) -> MutexGuard<'a, T> {
        self.inner.as_ref().unwrap().lock().await
    }
}
