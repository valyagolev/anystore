use std::sync::Arc;

use futures::{stream, StreamExt, TryStreamExt};
use tokio::sync::{RwLock, RwLockReadGuard};

use serde_json::Value;

use crate::{
    address::{
        primitive::Existence,
        traits::{
            AddressableInsert, AddressableList, AddressableRead, AddressableTree, AddressableWrite,
            BranchOrLeaf,
        },
        Address, Addressable, SubAddress,
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

    async fn lock_read_value(&self) -> StoreResult<(RwLockReadGuard<()>, Value), Self>
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

        let str = loc.get::<String>().await?;

        // .map_err(LocatedJsonStoreError::StoreError)
        let mut value = str
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or(Value::Null);

        let result = mutator(&mut value);

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
                // Set
                Some(value) => {
                    let insert_at = get_mut_pathvalue(cur, &addr[..], true)?.unwrap();

                    *insert_at = value.clone();

                    Ok(())
                }

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
            }
        })
        .await?
    }
}

impl<A: Address, S: AddressableRead<String, A>> AddressableRead<Existence, JsonPath>
    for LocatedJsonStore<A, S>
where
    <S as Store>::Error: std::error::Error,
{
    async fn read(&self, addr: &JsonPath) -> StoreResult<Option<Existence>, Self> {
        let v: Option<Value> =
            <LocatedJsonStore<A, S> as AddressableRead<Value, JsonPath>>::read(self, addr).await?;

        Ok(v.map(|_| Existence))
    }
}

impl<'a, A: Address, S: 'a + AddressableRead<String, A>> AddressableList<'a, JsonPath>
    for LocatedJsonStore<A, S>
where
    S::Error: std::error::Error,
{
    type AddedAddress = JsonPathPart;

    type ItemAddress = JsonPath;

    fn list(&self, addr: &JsonPath) -> Self::ListOfAddressesStream {
        let this = self.clone();
        let addr = addr.clone();

        stream::once(async move {
            let value = this.lock_read_value().await?.1;

            let val: StoreResult<_, Self> =
                try { get_pathvalue(&value, &addr.0[..])?.ok_or(anyhow!("Path doesn't exist"))? };

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
                _ => vec![Err(anyhow!("Can't list: {val:?}"))],
            };

            Ok::<_, Self::Error>(stream::iter(vec.into_iter()))
        })
        .try_flatten()
        .boxed_local()
    }
}

impl<'a, A: Address, S: 'a + AddressableRead<String, A>> AddressableTree<'a, JsonPath, JsonPath>
    for LocatedJsonStore<A, S>
where
    S::Error: std::error::Error,
{
    async fn branch_or_leaf(
        &self,
        addr: JsonPath,
    ) -> StoreResult<BranchOrLeaf<JsonPath, JsonPath>, Self> {
        let value = self.lock_read_value().await?.1;
        let val = get_pathvalue(&value, &addr.0[..])?.ok_or(anyhow!("Path doesn't exist"))?;

        Ok(match val {
            Value::Array(_) => BranchOrLeaf::Branch(addr),
            Value::Object(_) => BranchOrLeaf::Branch(addr),

            _ => BranchOrLeaf::Leaf(addr),
        })
    }
}

impl<'a, A: Address, S: 'a + AddressableRead<String, A> + AddressableWrite<String, A>>
    AddressableInsert<'a, Value, JsonPath> for LocatedJsonStore<A, S>
where
    S::Error: std::error::Error,
{
    fn insert(&self, addr: &JsonPath, items: Vec<Value>) -> Self::ListOfAddressesStream {
        let addr = addr.clone();
        let this = self.clone();

        stream::once(async move {
            let addr = addr.clone();
            let path = addr.0.clone();
            let paths = this
                .change_value(move |cur| {
                    let insert_at = get_mut_pathvalue(cur, &path[..], true)?.unwrap();

                    if insert_at.is_null() {
                        *insert_at = Value::Array(vec![]);
                    }

                    let arr = match insert_at {
                        Value::Array(at) => at,
                        _ => {
                            return Err::<_, Self::Error>(anyhow!(
                                "Can't insert into non-array value"
                            ))
                        }
                    };

                    let ixes = arr.len()..arr.len() + items.len();

                    arr.extend(items);

                    Ok(ixes
                        .map(JsonPathPart::Index)
                        .map(move |i| (i.clone(), addr.clone().sub(i))))
                })
                .await??;

            Ok::<_, Self::Error>(stream::iter(paths.map(Ok)))
        })
        .try_flatten()
        .boxed_local()
    }
}

#[cfg(test)]
#[cfg(feature = "json")]
mod test {
    use serde_json::json;

    use crate::{store::StoreEx, stores::json::json_value_store};
    use futures::TryStreamExt;

    #[tokio::test]
    async fn test() -> Result<(), anyhow::Error> {
        let root = json_value_store(json!({
            "test": {"a": 2},
            "list": [{"a":8}, {"b":2}, {"a": 3}]
        }))?
        .root();

        let vc: Vec<_> = root
            .clone()
            .path("list")?
            .insert(vec![json!({"a": 1}), json!({"b": 2}), json!({"a": 3})])
            .try_collect()
            .await?;

        assert_eq!(vc.len(), 3);
        assert_eq!(vc[0].0.to_string(), "[3]");
        assert_eq!(vc[1].1.to_string(), "list[4]");

        let vc: Vec<_> = root
            .path("test.deeper")?
            .insert(vec![json!({"a": 1}), json!({"b": 2})])
            .try_collect()
            .await?;

        assert_eq!(vc.len(), 2);
        assert_eq!(vc[0].0.to_string(), "[0]");
        assert_eq!(vc[1].1.to_string(), "test.deeper[1]");

        Ok(())
    }
}
