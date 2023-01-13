use std::sync::Arc;

use crate::{
    address::{
        primitive::Existence,
        traits::{
            tree::{AddressableTree, BranchOrLeaf},
            AddressableList, AddressableRead, AddressableWrite,
        },
        Addressable, SubAddress,
    },
    store::{Store, StoreResult},
};
use derive_more::{Display, From};
use futures::stream;
use parking_lot::RwLock;
use serde_json::Value;
use thiserror::Error;

pub mod paths;
pub(crate) mod traverse;

pub use paths::*;
use traverse::*;

#[derive(From, Display, Debug, Error)]
pub enum JsonValueStoreError {
    Serde(serde_json::Error),
    TaverseError(JsonTraverseError),
    ParseError(JsonPathParseError),
    Custom(String),
}

#[derive(Clone)]
pub struct JsonValueStore {
    value: Arc<RwLock<Value>>,
}

impl JsonValueStore {
    pub fn new(value: Value) -> Self {
        JsonValueStore {
            value: Arc::new(RwLock::new(value)),
        }
    }
    pub fn try_destruct(mut self) -> Result<Value, Self> {
        match Arc::try_unwrap(self.value) {
            Ok(v) => Ok(v.into_inner()),
            Err(this) => {
                self.value = this;
                Err(self)
            }
        }
    }
}

impl Store for JsonValueStore {
    type Error = JsonValueStoreError;
    type RootAddress = JsonPath;
}

impl Addressable<JsonPath> for JsonValueStore {
    type DefaultValue = Value;
}

impl AddressableRead<Value, JsonPath> for JsonValueStore {
    async fn read(&self, addr: &JsonPath) -> StoreResult<Option<Value>, Self> {
        let value = self.value.read();

        return Ok(get_pathvalue(&value, &addr.0[..])?.cloned());
    }
}

impl AddressableWrite<Value, JsonPath> for JsonValueStore {
    async fn write(
        &self,
        addr: &JsonPath,
        value: &Option<Value>,
    ) -> StoreResult<(), JsonValueStore> {
        let mut cur = self.value.write();
        let addr = &addr.0;

        return match value {
            // Delete
            None => {
                let Some((last, path)) = addr.split_last() else {
                    *cur = Value::Null;
                    return Ok(());
                };

                let delete_from = get_mut_pathvalue(&mut cur, path, false)?;

                match delete_from {
                    None => Ok(()),
                    Some(Value::Null) => Ok(()),

                    Some(delete_from) => match (last, delete_from) {
                        (JsonPathPart::Key(key), Value::Object(obj)) => {
                            obj.remove(key);
                            Ok(())
                        }
                        (JsonPathPart::Index(ix), Value::Array(arr)) => {
                            if arr.len() <= *ix {
                            } else if arr.len() == *ix {
                                arr.pop();
                            } else {
                                arr[*ix] = Value::Null;
                            }

                            Ok(())
                        }
                        (_, value) => {
                            Err(format!("Incompatible value at key {last}: {value}",).into())
                        }
                    },
                }
            }
            // Set
            Some(value) => {
                let insert_at = get_mut_pathvalue(&mut cur, &addr[..], true)?.unwrap();

                *insert_at = value.clone();

                Ok(())
            }
        };
    }
}

impl AddressableRead<Existence, JsonPath> for JsonValueStore {
    async fn read(&self, addr: &JsonPath) -> StoreResult<Option<Existence>, Self> {
        let value = self.value.read();
        let val = get_pathvalue(&value, &addr.0[..])?;
        Ok(val.map(|_| Default::default()))
    }
}

impl<'a> AddressableList<'a, JsonPath> for JsonValueStore {
    type AddedAddress = JsonPathPart;

    type ItemAddress = JsonPath;

    type ListOfAddressesStream =
        stream::Iter<std::vec::IntoIter<StoreResult<(JsonPathPart, JsonPath), JsonValueStore>>>;

    fn list(&self, addr: &JsonPath) -> Self::ListOfAddressesStream {
        let value = self.value.read();

        let val: StoreResult<_, JsonValueStore> =
            try { get_pathvalue(&value, &addr.0[..])?.ok_or("Path doesn't exist".to_owned())? };

        let vec = match val {
            Ok(Value::Array(arr)) => (0..arr.len())
                .map(JsonPathPart::Index)
                .map(|i| Ok((i.clone(), addr.clone().sub(i))))
                .collect(),
            Ok(Value::Object(obj)) => obj
                .keys()
                .map(|k| JsonPathPart::Key(k.to_owned()))
                .map(|i| Ok((i.clone(), addr.clone().sub(i))))
                .collect(),
            Err(e) => vec![Err(e)],
            _ => vec![Err(format!("Can't list: {val:?}").into())],
        };

        stream::iter(vec.into_iter())
    }
}

impl<'a> AddressableTree<'a, JsonPath, JsonPath> for JsonValueStore {
    async fn branch_or_leaf(
        &self,
        addr: JsonPath,
    ) -> StoreResult<BranchOrLeaf<JsonPath, JsonPath>, Self> {
        let value = self.value.read();
        let val = get_pathvalue(&value, &addr.0[..])?.ok_or("Path doesn't exist".to_owned())?;

        Ok(match val {
            Value::Array(_) => BranchOrLeaf::Branch(addr),
            Value::Object(_) => BranchOrLeaf::Branch(addr),

            _ => BranchOrLeaf::Leaf(addr),
        })
    }
}

// todo: how to make this automatic?
// mb create a "wrapper error" struct...
// ... or let a store handle this...
impl From<paths::JsonPathParseError>
    for crate::wrappers::filter_addresses::FilterAddressesWrapperError<
        crate::stores::json::JsonValueStoreError,
    >
{
    fn from(value: paths::JsonPathParseError) -> Self {
        JsonValueStoreError::from(value).into()
    }
}

#[cfg(test)]
mod test_tree {

    use futures::TryStreamExt;

    use serde_json::{json, Value};

    use crate::{store::StoreEx, wrappers::tree::filter_addresses::FilterAddressesWrapperStore};

    use super::paths::*;
    use super::JsonValueStore;
    use crate::address::primitive::UniqueRootAddress;
    use crate::address::SubAddress;

    #[tokio::test]
    pub async fn test_json_tree() -> Result<(), Box<dyn std::error::Error>> {
        let base_store = JsonValueStore::new(json!({
                "wow": {"hello": "yes"},
                "another": {"seriously": {"throrougly": 7}, "basic": [1,2,3,{"hello": "_why"},{"_why": "ya"}]},
                "_ignore": {"haha": {"_yes": 3}}
        }));
        let _store = base_store.clone();
        let store =
            FilterAddressesWrapperStore::new(base_store.clone(), |s: String| !s.starts_with('_'));
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
        println!("{:?}", base_store.value.read());
        assert_eq!(true, some.exists().await?);

        some.write(&Some(Value::Null)).await?;
        println!("{:?}", base_store.value.read());
        assert_eq!(true, some.exists().await?);

        some.write(&None).await?;
        println!("{:?}", base_store.value.read());
        assert_eq!(false, some.exists().await?);

        println!(
            "list base {:?}",
            base_store.root().list().try_collect::<Vec<_>>().await?
        );
        println!("list {:?}", root.list().try_collect::<Vec<_>>().await?);

        // root.list_tree(0).await;

        // Err("seems fine".to_owned().into())
        Ok(())
    }
}
