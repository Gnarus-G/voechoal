use std::{
    ops::Deref,
    sync::{Arc, Mutex},
};

#[repr(transparent)]
#[derive(Debug)]
pub struct SharedMutRef<T>(Arc<Mutex<T>>);

impl<T> Deref for SharedMutRef<T> {
    type Target = Arc<Mutex<T>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> SharedMutRef<T> {
    pub fn new(value: T) -> Self {
        Self(Arc::new(Mutex::new(value)))
    }

    pub fn new_ref(&self) -> Arc<Mutex<T>> {
        Arc::clone(&self.0)
    }
}
