use std::sync::Arc;

use crate::{
    address::{
        primitive::{Existence, UniqueRootAddress},
        traits::{
            AddressableList, AddressableRead, AddressableTree, AddressableWrite, BranchOrLeaf,
        },
        Addressable, SubAddress,
    },
    store::{Store, StoreEx, StoreResult},
    wrappers::filter_addresses::FilterAddressesWrapperError,
};
use derive_more::{Display, From};
use futures::{stream, StreamExt};
use serde_json::Value;
use thiserror::Error;
use tokio::sync::RwLock;

pub mod paths;
pub(crate) mod traverse;

pub use paths::*;
use traverse::*;

use super::{cell::MemoryCellStore, located::json::LocatedJsonStore};

// #[derive(From, Display, Debug, Error)]
// pub enum JsonValueStoreError {
//     Serde(serde_json::Error),
//     TaverseError(JsonTraverseError),
//     ParseError(JsonPathParseError),
//     Custom(String),
// }

// #[derive(Clone)]
// pub struct JsonValueStore {
//     value: Arc<RwLock<Value>>,
// }

// impl JsonValueStore {
//     pub fn new(value: Value) -> Self {
//         JsonValueStore {
//             value: Arc::new(RwLock::new(value)),
//         }
//     }
//     pub fn try_destruct(mut self) -> Result<Value, Self> {
//         match Arc::try_unwrap(self.value) {
//             Ok(v) => Ok(v.into_inner()),
//             Err(this) => {
//                 self.value = this;
//                 Err(self)
//             }
//         }
//     }
// }

// impl Store for JsonValueStore {
//     type Error = JsonValueStoreError;
//     type RootAddress = JsonPath;
// }

// impl Addressable<JsonPath> for JsonValueStore {
//     type DefaultValue = Value;
// }

// impl AddressableRead<Value, JsonPath> for JsonValueStore {
//     async fn read(&self, addr: &JsonPath) -> StoreResult<Option<Value>, Self> {
//         let value = self.value.read().await;

//         return Ok(get_pathvalue(&value, &addr.0[..])?.cloned());
//     }
// }

// impl AddressableWrite<Value, JsonPath> for JsonValueStore {
//     async fn write(
//         &self,
//         addr: &JsonPath,
//         value: &Option<Value>,
//     ) -> StoreResult<(), JsonValueStore> {
//         let mut cur = self.value.write().await;
//         let addr = &addr.0;

//         return match value {
//             // Delete
//             None => {
//                 let Some((last, path)) = addr.split_last() else {
//                     *cur = Value::Null;
//                     return Ok(());
//                 };

//                 let delete_from = get_mut_pathvalue(&mut cur, path, false)?;

//                 match delete_from {
//                     None => Ok(()),
//                     Some(Value::Null) => Ok(()),

//                     Some(delete_from) => match (last, delete_from) {
//                         (JsonPathPart::Key(key), Value::Object(obj)) => {
//                             obj.remove(key);
//                             Ok(())
//                         }
//                         (JsonPathPart::Index(ix), Value::Array(arr)) => {
//                             if arr.len() <= *ix {
//                             } else if arr.len() == *ix {
//                                 arr.pop();
//                             } else {
//                                 arr[*ix] = Value::Null;
//                             }

//                             Ok(())
//                         }
//                         (_, value) => {
//                             Err(format!("Incompatible value at key {last}: {value}",).into())
//                         }
//                     },
//                 }
//             }
//             // Set
//             Some(value) => {
//                 let insert_at = get_mut_pathvalue(&mut cur, &addr[..], true)?.unwrap();

//                 *insert_at = value.clone();

//                 Ok(())
//             }
//         };
//     }
// }

// impl AddressableRead<Existence, JsonPath> for JsonValueStore {
//     async fn read(&self, addr: &JsonPath) -> StoreResult<Option<Existence>, Self> {
//         let value = self.value.read().await;
//         let val = get_pathvalue(&value, &addr.0[..])?;
//         Ok(val.map(|_| Default::default()))
//     }
// }

// todo: how to make this automatic?
// mb create a "wrapper error" struct...
// ... or let a store handle this...
impl From<paths::JsonPathParseError>
    for crate::wrappers::filter_addresses::FilterAddressesWrapperError<
        FilterAddressesWrapperError<anyhow::Error>,
    >
{
    fn from(value: paths::JsonPathParseError) -> Self {
        FilterAddressesWrapperError::StoreError(FilterAddressesWrapperError::StoreError(
            value.into(),
        ))
    }
}

impl From<JsonPathParseError> for FilterAddressesWrapperError<anyhow::Error> {
    fn from(value: JsonPathParseError) -> Self {
        FilterAddressesWrapperError::StoreError(value.into())
    }
}

pub fn json_value_store(
    val: Value,
) -> serde_json::Result<LocatedJsonStore<UniqueRootAddress, MemoryCellStore<String>>> {
    let cell_store = MemoryCellStore::new(Some(serde_json::to_string(&val)?));

    Ok(LocatedJsonStore::new(cell_store.root()))
}

#[cfg(test)]
mod test_tree {

    use futures::TryStreamExt;

    use serde_json::{json, Value};

    use crate::stores::cell::MemoryCellStore;
    use crate::stores::json::json_value_store;
    use crate::stores::located::json::LocatedJsonStore;
    use crate::{store::StoreEx, wrappers::filter_addresses::FilterAddressesWrapperStore};

    use super::paths::*;
    use crate::address::primitive::UniqueRootAddress;
    use crate::address::SubAddress;

    #[tokio::test]
    pub async fn test_json_tree() -> Result<(), Box<dyn std::error::Error>> {
        let val = json!({
                "wow": {"hello": "yes"},
                "another": {"seriously": {"throrougly": 7}, "basic": [1,2,3,{"hello": "_why"},{"_why": "ya"}]},
                "_ignore": {"haha": {"_yes": 3}}
        });

        let cell_store = MemoryCellStore::new(Some(serde_json::to_string(&val)?));
        let json_store = LocatedJsonStore::new(cell_store.root());

        let store =
            FilterAddressesWrapperStore::new(json_store.clone(), |s: String| !s.starts_with('_'));
        let root = store.root();

        let some = root.clone().sub(
            JsonPath::from(UniqueRootAddress)
                .sub(JsonPathPart::Key("wow".to_owned()))
                .sub(JsonPathPart::Key("hello".to_owned())),
        );
        println!("wow: {:?}", root.clone().path("wow")?.get::<Value>().await?);
        println!(
            "wow.hello: {:?}",
            root.clone().path("wow.hello")?.get::<Value>().await?
        );
        println!(
            "wow.hello: {:?}",
            root.clone().path("wow.hello")?.getv().await?
        );
        println!("{:?}", root.clone().path("wow")?.path("hello")?.address);
        println!(
            "{:?}",
            root.clone()
                .path("wow")?
                .path("_hello")?
                .get::<Value>()
                .await?
        );
        println!(
            "wow->hello: {:?}",
            root.clone()
                .path("wow")?
                .path("hello")?
                .get::<Value>()
                .await?
        );

        println!("some {:?}", some.get::<Value>().await?);
        println!("root {:?}", root.get::<Value>().await?);

        assert_eq!(some.get().await?, Some(json!("yes")));

        some.write(&Some(json!("no"))).await?;
        println!("{:?}", cell_store.root().getv().await);
        assert_eq!(true, some.exists().await?);

        some.write(&Some(Value::Null)).await?;
        println!("{:?}", cell_store.root().getv().await);
        assert_eq!(true, some.exists().await?);

        some.write(&None).await?;
        println!("{:?}", cell_store.root().getv().await);
        assert_eq!(false, some.exists().await?);

        println!(
            "list base {:?}",
            json_store.root().list().try_collect::<Vec<_>>().await?
        );
        println!("list {:?}", root.list().try_collect::<Vec<_>>().await?);

        // root.list_tree(0).await;

        // Err("seems fine".to_owned().into())
        Ok(())
    }
}
