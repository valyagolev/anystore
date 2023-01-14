use std::{collections::HashMap, sync::Arc, time::Duration};

use derive_more::{Display, From};

use futures::{stream, Stream, StreamExt, TryStreamExt};
use reqwest::Method;
use serde_json::Value;
use thiserror::Error;

use crate::{
    address::{
        traits::{AddressableList, AddressableQuery},
        Address, Addressable, SubAddress,
    },
    store::Store,
    util::ratelimiter::Ratelimiter,
};

#[derive(From, Display, Debug, Error)]
pub enum AirtableStoreError {
    Custom(String),
    HttpError(reqwest::Error),
    JsonError(serde_json::Error),
}

impl<'a> From<&'a str> for AirtableStoreError {
    fn from(value: &'a str) -> Self {
        AirtableStoreError::Custom(value.to_owned())
    }
}

#[derive(Clone)]
pub struct AirtableStore {
    http_client: reqwest::Client,
    ratelimiter: Arc<Ratelimiter>,
}

impl AirtableStore {
    pub fn new(token: &str) -> Result<Self, AirtableStoreError> {
        let headers = (&HashMap::from([("Authorization".to_owned(), format!("Bearer {token}"))]))
            .try_into()
            .map_err(|_| "invalid token")?;

        Ok(AirtableStore {
            http_client: reqwest::Client::builder()
                .default_headers(headers)
                .build()?,
            ratelimiter: Arc::new(Ratelimiter::new(Duration::from_secs(1), 5)),
        })
    }

    async fn get_json(
        &self,
        url: &str,
        query: HashMap<String, String>,
    ) -> Result<Value, AirtableStoreError> {
        self.ratelimiter.ask().await;

        let val = self
            .http_client
            .request(Method::GET, url)
            .query(&query)
            .send()
            .await?
            .text()
            .await?;

        Ok(serde_json::from_str(&val)?)
    }

    fn get_paginated(
        &self,
        url: &str,
        object_key: &str,
        query: HashMap<String, String>,
    ) -> impl Stream<Item = Result<(String, Value), AirtableStoreError>> {
        let this = self.clone();
        let object_key = object_key.to_owned();
        let url = url.to_owned();
        // let query = query.clone();

        stream::try_unfold(Some("".to_owned()), move |next_offset| {
            let this = this.clone();
            let object_key = object_key.clone();
            let url = url.clone();
            let query = query.clone();

            async move {
                let Some(next_offset) = next_offset else {
                    return Ok(None);
                };

                let mut paged_q = query.clone();
                paged_q.insert("offset".to_owned(), next_offset);

                let resp = this.get_json(&url, paged_q).await?;

                let bases = resp
                    .get(&object_key)
                    .ok_or(format!("No {object_key} in resp"))?
                    .as_array()
                    .ok_or("Bad obj list type")?
                    .iter()
                    .map(|v| Some((v.get("id")?.as_str()?.to_owned(), v.clone())))
                    .collect::<Option<Vec<_>>>()
                    .ok_or("Api conversion problem")?;

                Ok::<_, AirtableStoreError>(Some((
                    bases,
                    resp.get("offset")
                        .and_then(|v| v.as_str().map(|s| s.to_owned())),
                )))
            }
        })
        .map_ok(|v| stream::iter(v.into_iter().map(Ok)))
        .try_flatten()
    }
}

impl Store for AirtableStore {
    type Error = AirtableStoreError;

    type RootAddress = crate::address::primitive::UniqueRootAddress;
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AirtableBasesRootAddr;

impl Address for AirtableBasesRootAddr {
    fn own_name(&self) -> String {
        "@bases".to_owned()
    }

    fn as_parts(&self) -> Vec<String> {
        vec![self.own_name()]
    }
}
impl Addressable<AirtableBasesRootAddr> for AirtableStore {
    type DefaultValue = AirtableBase;
}

impl SubAddress<AirtableBase> for AirtableBasesRootAddr {
    type Output = AirtableBase;

    fn sub(self, rhs: AirtableBase) -> Self::Output {
        rhs
    }
}

impl<'a> AddressableList<'a, AirtableBasesRootAddr> for AirtableStore {
    type AddedAddress = AirtableBase;

    type ItemAddress = AirtableBase;

    fn list(&self, _addr: &AirtableBasesRootAddr) -> Self::ListOfAddressesStream {
        self.get_paginated(
            "https://api.airtable.com/v0/meta/bases",
            "bases",
            Default::default(),
        )
        .map(|v| {
            let (id, value) = v?;
            let b = AirtableBase {
                id,
                meta: serde_json::from_value(value)?,
            };
            Ok((b.clone(), b))
        })
        .boxed_local()
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AirtableBase {
    pub id: String,
    pub meta: Option<Value>,
}

impl AirtableBase {
    fn by_id(id: &str) -> Self {
        AirtableBase {
            id: id.to_owned(),
            meta: None,
        }
    }
}

impl Address for AirtableBase {
    fn own_name(&self) -> String {
        self.id.to_string()
    }

    fn as_parts(&self) -> Vec<String> {
        vec![self.own_name()]
    }
}
impl Addressable<AirtableBase> for AirtableStore {}
impl SubAddress<AirtableTable> for AirtableBase {
    type Output = AirtableTable;

    fn sub(self, mut rhs: AirtableTable) -> Self::Output {
        // TODO: not a good sign... how do we do it?
        match &rhs.base {
            Some(b) => assert_eq!(b, &self),
            None => {
                rhs.base = Some(self);
            }
        }

        rhs
    }
}

impl<'a> AddressableList<'a, AirtableBase> for AirtableStore {
    type AddedAddress = AirtableTable;

    type ItemAddress = AirtableTable;

    fn list(&self, addr: &AirtableBase) -> Self::ListOfAddressesStream {
        let addr = addr.clone();

        self.get_paginated(
            &format!("https://api.airtable.com/v0/meta/bases/{}/tables", addr.id),
            "tables",
            Default::default(),
        )
        .map(move |v| {
            let (id, value) = v?;
            let b = AirtableTable {
                id,
                base: Some(addr.clone()),
                meta: serde_json::from_value(value)?,
            };
            Ok((b.clone(), b))
        })
        .boxed_local()
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AirtableTable {
    pub id: String,
    pub base: Option<AirtableBase>,
    pub meta: Option<Value>,
}

impl AirtableTable {
    fn by_id_or_name(id_or_name: &str) -> Self {
        AirtableTable {
            id: id_or_name.to_owned(),
            base: None,
            meta: None,
        }
    }
}

impl Address for AirtableTable {
    fn own_name(&self) -> String {
        self.id.to_owned()
    }

    fn as_parts(&self) -> Vec<String> {
        let base_id = self
            .base
            .as_ref()
            .map(|b| b.id.to_owned())
            .unwrap_or("(unknown base)".to_owned());

        vec![base_id, self.id.to_owned()]
    }
}
impl Addressable<AirtableTable> for AirtableStore {}

// TODO: id/value stuff is a bit of a boilerplate
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AirtableRecord {
    pub id: String,
    pub table: AirtableTable,
    pub value: Option<Value>,
}

impl SubAddress<AirtableRecord> for AirtableTable {
    type Output = AirtableRecord;

    fn sub(self, rhs: AirtableRecord) -> Self::Output {
        assert!(self == rhs.table);

        rhs
    }
}

impl Address for AirtableRecord {
    fn own_name(&self) -> String {
        self.id.to_owned()
    }

    fn as_parts(&self) -> Vec<String> {
        let mut v = self.table.as_parts();
        v.push(self.id.to_owned());
        v
    }
}
impl Addressable<AirtableRecord> for AirtableStore {
    type DefaultValue = Value;
}

impl<'a> AddressableList<'a, AirtableTable> for AirtableStore {
    type AddedAddress = AirtableRecord;

    type ItemAddress = AirtableRecord;

    fn list(&self, addr: &AirtableTable) -> Self::ListOfAddressesStream {
        let addr = addr.clone();
        let this = self.clone();

        stream::once(async move {
            let addr = addr.clone();
            let addr2 = addr.clone();

            let s = this
                .get_paginated(
                    &format!(
                        "https://api.airtable.com/v0/{}/{}",
                        addr.base
                            .ok_or(AirtableStoreError::Custom(
                                "Table address contains no base address".to_owned()
                            ))?
                            .id,
                        addr.id
                    ),
                    "records",
                    Default::default(),
                )
                .map(move |v| {
                    let (id, value) = v?;
                    let b = AirtableRecord {
                        id,
                        table: addr2.clone(),
                        value: serde_json::from_value(value)?,
                    };
                    Ok((b.clone(), b))
                });

            Ok::<_, AirtableStoreError>(s)
        })
        .try_flatten()
        .boxed_local()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FilterByFormula(pub String);

impl<'a> AddressableQuery<'a, FilterByFormula, AirtableTable> for AirtableStore {
    fn query(&self, addr: &AirtableTable, query: FilterByFormula) -> Self::ListOfAddressesStream {
        let addr = addr.clone();
        let this = self.clone();

        stream::once(async move {
            let addr = addr.clone();
            let addr2 = addr.clone();

            let s = this
                .get_paginated(
                    &format!(
                        "https://api.airtable.com/v0/{}/{}",
                        addr.base
                            .ok_or(AirtableStoreError::Custom(
                                "Table address contains no base address".to_owned()
                            ))?
                            .id,
                        addr.id
                    ),
                    "records",
                    HashMap::from_iter([("filterByFormula".to_owned(), query.0)]),
                )
                .map(move |v| {
                    let (id, value) = v?;
                    let b = AirtableRecord {
                        id,
                        table: addr2.clone(),
                        value: serde_json::from_value(value)?,
                    };
                    Ok((b.clone(), b))
                });

            Ok::<_, AirtableStoreError>(s)
        })
        .try_flatten()
        .boxed_local()
    }
}

#[cfg(test)]
mod test_airtable {
    use crate::{
        store::StoreEx,
        stores::cloud::airtable::{
            AirtableBase, AirtableBasesRootAddr, AirtableStore, AirtableTable, FilterByFormula,
        },
    };
    use futures::StreamExt;

    #[tokio::test]
    #[ignore]
    pub async fn test_airtable() -> Result<(), Box<dyn std::error::Error>> {
        let store =
            AirtableStore::new(&std::env::var("AIRTABLE_API_KEY").expect("AIRTABLE_API_KEY"))?;

        let mut bases = store.sub(AirtableBasesRootAddr).list();
        while let Some(b) = bases.next().await {
            let (b, _) = b?;
            println!("{:?}", b.meta.clone().unwrap()["name"].as_str().unwrap());

            let mut tables = store.sub(b).list();
            while let Some(t) = tables.next().await {
                let (t, _) = t?;
                println!(
                    "    {:?}",
                    t.meta.clone().unwrap()["name"].as_str().unwrap()
                );

                let mut records = store.sub(t).list();

                while let Some(r) = records.next().await {
                    let (r, _) = r?;
                    println!("    {:?}", r.value.unwrap());
                    // print!(".");
                }
            }
        }

        println!("");
        println!("");
        println!("Will query...");

        let mut query = store
            .sub(AirtableBase::by_id("app46Mmalo62fN5Vq"))
            .sub(AirtableTable::by_id_or_name("Entries"))
            .query(FilterByFormula("Find(\"RPC\", {title})".to_owned()));

        while let Some(v) = query.next().await {
            let (v, _) = v?;
            println!("    {:?}", v.value.unwrap());
        }

        Ok(())
        // Err(AirtableStoreError::Custom("lol".to_owned()))?
    }
}
