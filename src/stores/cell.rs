use std::sync::Arc;

use thiserror::Error;
use tokio::sync::RwLock;

use crate::{
    address::{
        primitive::UniqueRootAddress,
        traits::{AddressableGet, AddressableSet},
        Addressable,
    },
    store::Store,
};

#[derive(Debug, Error, Eq, PartialEq)]
pub enum MemoryCellStoreError {}

#[derive(Debug, Clone)]
pub struct MemoryCellStore<V: Clone> {
    value: Arc<RwLock<Option<V>>>,
}

impl<V: Clone> MemoryCellStore<V> {
    pub fn new(value: Option<V>) -> Self {
        MemoryCellStore {
            value: Arc::new(RwLock::new(value)),
        }
    }
}

impl<V: Clone> Store for MemoryCellStore<V> {
    type Error = MemoryCellStoreError;
}

impl<V: Clone> Addressable<UniqueRootAddress> for MemoryCellStore<V> {
    type DefaultValue = V;
}

impl<V: Clone> AddressableGet<V, UniqueRootAddress> for MemoryCellStore<V> {
    async fn addr_get(&self, _address: &UniqueRootAddress) -> Result<Option<V>, Self::Error> {
        let value = self.value.read().await.clone();
        Ok(value)
    }
}

impl<V: Clone> AddressableSet<V, UniqueRootAddress> for MemoryCellStore<V> {
    async fn write(
        &self,
        _address: &UniqueRootAddress,
        value: &Option<V>,
    ) -> Result<(), Self::Error> {
        *self.value.write().await = value.clone();
        Ok(())
    }
}
