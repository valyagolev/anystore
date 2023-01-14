use std::sync::Arc;

use derive_more::From;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::{
    address::{traits::AddressableGet, Address, Addressable},
    store::Store,
};

#[derive(From, Debug, Error)]
pub enum IndexedVecStoreError {}

pub struct IndexedVecStore<
    V: Clone,
    IdType: ToString + PartialEq + Eq + std::fmt::Debug + Clone,
    F: Fn(&V) -> IdType,
> {
    vec: RwLock<Vec<V>>,
    get_id: F,
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Id<IdType>(IdType);

impl<IdType: ToString + PartialEq + Eq + std::fmt::Debug + Clone + 'static> Address for Id<IdType> {
    fn own_name(&self) -> String {
        self.0.to_string()
    }

    fn as_parts(&self) -> Vec<String> {
        vec![self.0.to_string()]
    }
}

impl<
        V: Clone,
        IdType: ToString + PartialEq + Eq + std::fmt::Debug + Clone,
        F: Fn(&V) -> IdType,
    > IndexedVecStore<V, IdType, F>
{
    pub fn new(vec: Vec<V>, get_id: F) -> Arc<Self> {
        Arc::new(IndexedVecStore {
            vec: RwLock::new(vec),
            get_id,
        })
    }
}

impl<
        V: Clone,
        IdType: ToString + PartialEq + Eq + std::fmt::Debug + Clone,
        F: Fn(&V) -> IdType,
    > Store for Arc<IndexedVecStore<V, IdType, F>>
{
    type Error = IndexedVecStoreError;

    type RootAddress = crate::address::primitive::UniqueRootAddress;
}
impl<
        V: Clone,
        IdType: ToString + PartialEq + Eq + std::fmt::Debug + Clone + 'static,
        F: Fn(&V) -> IdType,
    > Addressable<Id<IdType>> for Arc<IndexedVecStore<V, IdType, F>>
{
    type DefaultValue = V;
}

impl<
        V: Clone,
        IdType: ToString + PartialEq + Eq + std::fmt::Debug + Clone + 'static,
        F: Fn(&V) -> IdType,
    > AddressableGet<V, Id<IdType>> for Arc<IndexedVecStore<V, IdType, F>>
{
    async fn read(&self, addr: &Id<IdType>) -> crate::store::StoreResult<Option<V>, Self> {
        Ok(self
            .vec
            .read()
            .await
            .iter()
            .find(|v| (self.get_id)(v) == addr.0)
            .cloned())
    }
}

#[cfg(test)]
#[cfg(feature = "json")]
mod test {
    use serde_json::json;

    use crate::{
        store::StoreEx,
        stores::indexed_vec::{Id, IndexedVecStore},
    };

    // TODO: make it a real wrapper

    #[tokio::test]
    async fn test() {
        let s = IndexedVecStore::new(
            vec![
                json!({"a": 1, "b": 2}),
                json!({"a": 3, "b": "z"}),
                json!({"a": 2, "b": "other"}),
            ],
            |v| v["a"].as_i64().unwrap(),
        );

        let v = s.sub(Id(5)).get().await;

        println!("{v:?}");

        // panic!("lol");
        // Ok(())
    }
}
