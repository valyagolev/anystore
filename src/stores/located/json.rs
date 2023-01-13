use std::sync::Arc;

use tokio::sync::{RwLock, RwLockReadGuard};

use serde_json::Value;

use crate::{
    address::{
        traits::{AddressableRead, AddressableWrite},
        Address, Addressable,
    },
    location::Location,
    store::{Store, StoreResult},
    stores::json::paths::*,
    stores::json::traverse::*,
};
// todo: stop using anyhow, implement wrapper error
use anyhow::anyhow;

// #[derive(Debug, Display, Error)]
type LocatedJsonStoreError = anyhow::Error;

// #[derive(Debug, Error)]
// pub enum LocatedJsonStoreError {
//     #[error("StoreError({0})")]
//     StoreError(dyn std::error::Error),
//     // #[error("CustomError({0})")]
//     // CustomError(String),

//     // #[error("SerdeError({0})")]
//     // SerdeError(
//     //     #[backtrace]
//     //     #[source]
//     //     serde_json::Error,
//     // ),

//     // #[error("ParseError({0})")]
//     // ParseError(
//     //     #[backtrace]
//     //     #[source]
//     //     JsonPathParseError,
//     // ),

//     // #[error("TraverseError({0})")]
//     // TraverseError(
//     //     #[backtrace]
//     //     #[source]
//     //     JsonTraverseError,
//     // ),
// }

/// Turn any store of Strings into JSON store
///
#[cfg_attr(not(all(feature = "json", feature = "fs")), doc = "```ignore")]
#[cfg_attr(all(feature = "json", feature = "fs"), doc = "```")]
/// use serde_json::json;
///
/// use anystore::stores::located::json::LocatedJsonStore;
/// use anystore::stores::fs::FileSystemStore;
///
/// use anystore::store::StoreEx;
/// use anystore::address::primitive::Existence;
///
///
/// # tokio_test::block_on(async {
///     let _ = tokio::fs::remove_file("test.json").await;
///
///     let fileloc = FileSystemStore::here()?.path("test.json")?;
///
///     assert_eq!(fileloc.get::<Existence>().await?, None);
///     assert_eq!(fileloc.get::<Existence>().await?, None);
///
///     let json_there = LocatedJsonStore::new(fileloc.clone());
///
///     let l = json_there.path("sub.key")?;
///
///     l.write(&Some(json!("wow"))).await?;
///
///     assert_eq!(fileloc.get::<Existence>().await?, Some(Existence));
///
///     assert_eq!(l.get().await?, Some(json!("wow")));
///
///     assert_eq!(fileloc.get::<String>().await?, Some(serde_json::to_string(&json!({"sub": {"key": "wow"}}))?));
///
///     tokio::fs::remove_file("test.json").await?;
///
/// #    Ok::<(), anyhow::Error>(())
/// # }).unwrap()
/// ```
#[derive(Clone)]
pub struct LocatedJsonStore<A: Address, S: Addressable<A>> {
    pub pretty: bool,

    location: Arc<RwLock<Location<A, S>>>,
}

impl<A: Address, S: Addressable<A>> LocatedJsonStore<A, S>
where
    S::Error: std::error::Error,
{
    /// Wrap a store of Strings into a JSON store
    pub fn new(location: Location<A, S>) -> Self {
        LocatedJsonStore {
            location: Arc::new(RwLock::new(location)),
            pretty: false,
        }
    }

    /// Wrap a store of Strings into a JSON store,
    /// that formats JSON with pretty print
    pub fn new_pretty(location: Location<A, S>) -> Self {
        LocatedJsonStore {
            location: Arc::new(RwLock::new(location)),
            pretty: true,
        }
    }

    async fn lock_read_value<'a>(&'a self) -> StoreResult<(RwLockReadGuard<'a, ()>, Value), Self>
    where
        S: AddressableRead<String, A>,
    {
        let loc = self.location.read().await;

        let value = loc
            .get::<String>()
            .await?
            // .map_err(LocatedJsonStoreError::StoreError)
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or(Value::Null);

        let lock = RwLockReadGuard::map(loc, |_| &());

        Ok((lock, value))
    }

    async fn change_value<R, F: FnOnce(&mut Value) -> R>(&self, mutator: F) -> StoreResult<R, Self>
    where
        S: AddressableRead<String, A> + AddressableWrite<String, A>,
    {
        let loc = self.location.write().await;

        let mut value = loc
            .get::<String>()
            .await?
            // .map_err(LocatedJsonStoreError::StoreError)
            .map(serde_json::to_value)
            .transpose()?
            .unwrap_or(Value::Null);

        let result = mutator(&mut value);

        println!("Will convert something from {value}");

        let stored = if self.pretty {
            serde_json::to_string_pretty(&value)
        } else {
            serde_json::to_string(&value)
        }?;

        loc.write(&Some(stored))
            .await
            // .map_err(LocatedJsonStoreError::StoreError)
            ?;

        Ok(result)
    }
}

impl<A: Address, S: Addressable<A>> Store for LocatedJsonStore<A, S> {
    type Error = LocatedJsonStoreError;
    type RootAddress = JsonPath;
}

impl<A: Address, S: Addressable<A>> Addressable<JsonPath> for LocatedJsonStore<A, S> {
    type DefaultValue = Value;
}

impl<A: Address, S: AddressableRead<String, A>> AddressableRead<Value, JsonPath>
    for LocatedJsonStore<A, S>
where
    <S as Store>::Error: std::error::Error,
{
    async fn read(&self, addr: &JsonPath) -> StoreResult<Option<Value>, Self> {
        let (_, value) = self.lock_read_value().await?;

        return Ok(get_pathvalue(&value, &addr.0[..])?
            // .map_err(LocatedJsonStoreError::TraverseError)
            .cloned());
    }
}

impl<A: Address, S: AddressableRead<String, A> + AddressableWrite<String, A>>
    AddressableWrite<Value, JsonPath> for LocatedJsonStore<A, S>
where
    <S as Store>::Error: std::error::Error,
{
    async fn write(&self, addr: &JsonPath, value: &Option<Value>) -> StoreResult<(), Self> {
        self.change_value(|cur| {
            let addr = &addr.0;

            match value {
                // Delete
                None => {
                    let Some((last, path)) = addr.split_last() else {
                    *cur = Value::Null;
                    return Ok(());
                };

                    let delete_from = get_mut_pathvalue(cur, path, false)?;

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
                                Err(anyhow!("Incompatible value at key {last}: {value}",))
                            }
                        },
                    }
                }
                // Set
                Some(value) => {
                    let insert_at = get_mut_pathvalue(cur, &addr[..], true)?.unwrap();

                    *insert_at = value.clone();

                    Ok(())
                }
            }
        })
        .await?
    }
}

// // impl AddressableRead<Existence, JsonPath> for LocatedJsonStore {
// //     async fn read(&self, addr: &JsonPath) -> StoreResult<Option<Existence>, Self> {
// //         let value = self.value.read();
// //         let val = get_pathvalue(&value, &addr.0[..])?;
// //         Ok(val.map(|_| Default::default()))
// //     }
// // }

// // impl<'a> AddressableList<'a, JsonPath> for LocatedJsonStore {
// //     type AddedAddress = JsonPathPart;

// //     type ItemAddress = JsonPath;

// //     type ListOfAddressesStream =
// //         stream::Iter<std::vec::IntoIter<StoreResult<(JsonPathPart, JsonPath), LocatedJsonStore>>>;

// //     fn list(&self, addr: &JsonPath) -> Self::ListOfAddressesStream {
// //         let value = self.value.read();

// //         let val: StoreResult<_, LocatedJsonStore> =
// //             try { get_pathvalue(&value, &addr.0[..])?.ok_or("Path doesn't exist".to_owned())? };

// //         let vec = match val {
// //             Ok(Value::Array(arr)) => (0..arr.len())
// //                 .map(JsonPathPart::Index)
// //                 .map(|i| Ok((i.clone(), addr.clone().sub(i))))
// //                 .collect(),
// //             Ok(Value::Object(obj)) => obj
// //                 .keys()
// //                 .map(|k| JsonPathPart::Key(k.to_owned()))
// //                 .map(|i| Ok((i.clone(), addr.clone().sub(i))))
// //                 .collect(),
// //             Err(e) => vec![Err(e)],
// //             _ => vec![Err(format!("Can't list: {val:?}").into())],
// //         };

// //         stream::iter(vec.into_iter())
// //     }
// // }

// // impl<'a> AddressableTree<'a, JsonPath, JsonPath> for LocatedJsonStore {
// //     async fn branch_or_leaf(
// //         &self,
// //         addr: JsonPath,
// //     ) -> StoreResult<BranchOrLeaf<JsonPath, JsonPath>, Self> {
// //         let value = self.value.read();
// //         let val = get_pathvalue(&value, &addr.0[..])?.ok_or("Path doesn't exist".to_owned())?;

// //         Ok(match val {
// //             Value::Array(_) => BranchOrLeaf::Branch(addr),
// //             Value::Object(_) => BranchOrLeaf::Branch(addr),

// //             _ => BranchOrLeaf::Leaf(addr),
// //         })
// //     }
// // }

// // impl From<JsonPath> for String {
// //     fn from(value: JsonPath) -> Self {
// //         value.to_string()
// //     }
// // }

// // impl From<JsonPathPart> for String {
// //     fn from(value: JsonPathPart) -> Self {
// //         value.to_string()
// //     }
// // }
// #[cfg(feature = "fs")]
// #[cfg(test)]
// mod test_tree {

//     use serde_json::json;

//     use crate::{address::primitive::Existence, store::StoreEx, stores::fs::FileSystemStore};

//     use super::LocatedJsonStore;

//     #[tokio::test]
//     pub async fn test_json_tree() -> Result<(), Box<dyn std::error::Error>> {
//         let _ = tokio::fs::remove_file("test.json").await;

//         let fileloc = FileSystemStore::here()?.path("test.json")?;

//         assert_eq!(fileloc.get::<Existence>().await?, None);
//         assert_eq!(fileloc.get::<Existence>().await?, None);

//         let json_there = LocatedJsonStore::new_pretty(fileloc.clone());

//         let l = json_there.path("sub.key")?;

//         l.write(&Some(json!("wow"))).await?;

//         assert_eq!(fileloc.get::<Existence>().await?, Some(Existence));

//         assert_eq!(l.get().await?, Some(json!("wow")));

//         tokio::fs::remove_file("test.json").await?;

//         Ok(())
//     }
// }
// //         let base_store = LocatedJsonStore::new(json!({
// //                 "wow": {"hello": "yes"},
// //                 "another": {"seriously": {"throrougly": 7}, "basic": [1,2,3,{"hello": "_why"},{"_why": "ya"}]},
// //                 "_ignore": {"haha": {"_yes": 3}}
// //         }));
// //         let _store = base_store.clone();
// //         let store =
// //             FilterAddressesWrapperStore::new(base_store.clone(), |s: String| !s.starts_with('_'));
// //         let root = store.root();

// //         let some = root.clone().sub(JsonPath(vec![
// //             JsonPathPart::Key("wow".to_owned()),
// //             JsonPathPart::Key("hello".to_owned()),
// //         ]));
// //         println!("wow: {:?}", root.clone().path("wow")?.get::<Value>().await?);
// //         println!(
// //             "wow.hello: {:?}",
// //             root.clone().path("wow.hello")?.get::<Value>().await?
// //         );
// //         println!(
// //             "wow.hello: {:?}",
// //             root.clone().path("wow.hello")?.getv().await?
// //         );
// //         println!("{:?}", root.clone().path("wow")?.path("hello")?.address);
// //         println!(
// //             "{:?}",
// //             root.clone()
// //                 .path("wow")?
// //                 .path("_hello")?
// //                 .get::<Value>()
// //                 .await?
// //         );
// //         println!(
// //             "wow->hello: {:?}",
// //             root.clone()
// //                 .path("wow")?
// //                 .path("hello")?
// //                 .get::<Value>()
// //                 .await?
// //         );

// //         println!("some {:?}", some.get::<Value>().await?);
// //         println!("root {:?}", root.get::<Value>().await?);

// //         assert_eq!(some.get().await?, Some(json!("yes")));

// //         some.write(&Some(json!("no"))).await?;
// //         println!("{:?}", base_store.value.read());
// //         assert_eq!(true, some.exists().await?);

// //         some.write(&Some(Value::Null)).await?;
// //         println!("{:?}", base_store.value.read());
// //         assert_eq!(true, some.exists().await?);

// //         some.write(&None).await?;
// //         println!("{:?}", base_store.value.read());
// //         assert_eq!(false, some.exists().await?);

// //         println!(
// //             "list base {:?}",
// //             base_store.root().list().try_collect::<Vec<_>>().await?
// //         );
// //         println!("list {:?}", root.list().try_collect::<Vec<_>>().await?);

// //         // root.list_tree(0).await;

// //         // Err("seems fine".to_owned().into())
// //         Ok(())
// //     }
// // }
