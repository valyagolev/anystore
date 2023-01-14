use crate::{
    address::primitive::UniqueRootAddress,
    store::{Store, StoreEx},
    wrappers::filter_addresses::FilterAddressesWrapperError,
};

use serde_json::Value;

pub mod paths;
pub(crate) mod traverse;

pub use paths::*;

use super::{cell::MemoryCellStore, located::json::LocatedJsonStore};

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

pub type JsonValueStore = LocatedJsonStore<UniqueRootAddress, MemoryCellStore<String>>;
pub type JsonValueStoreError = <JsonValueStore as Store>::Error;

pub fn json_value_store(val: Value) -> Result<JsonValueStore, JsonValueStoreError> {
    let cell_store = MemoryCellStore::new(Some(serde_json::to_string(&val)?));

    Ok(LocatedJsonStore::new(cell_store.root()))
}

#[cfg(test)]
mod test_tree {

    use futures::TryStreamExt;

    use serde_json::{json, Value};

    use crate::stores::cell::MemoryCellStore;

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

        some.set(&Some(json!("no"))).await?;
        println!("{:?}", cell_store.root().getv().await);
        assert_eq!(true, some.exists().await?);

        some.set(&Some(Value::Null)).await?;
        println!("{:?}", cell_store.root().getv().await);
        assert_eq!(true, some.exists().await?);

        some.set(&None).await?;
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
