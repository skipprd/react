use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct CapabilityMap {
    inner: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl CapabilityMap {
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.inner
            .get(&TypeId::of::<T>())
            .and_then(|a| a.clone().downcast::<T>().ok())
    }

    pub fn set<T: Send + Sync + 'static>(&mut self, val: Arc<T>) {
        self.inner.insert(TypeId::of::<T>(), val);
    }
}
