use std::{collections::HashMap, fmt::Formatter, marker::PhantomData, sync::Arc, time::Duration};

use derive_more::{Display, From};

use futures::{stream, Stream, StreamExt, TryStreamExt};
use reqwest::Method;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{from_value, json, Value};
use std::fmt::Debug;
use thiserror::Error;

use crate::{
    address::{
        traits::{AddressableInsert, AddressableList, AddressableQuery},
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
        let headers = (&HashMap::from([
            ("Authorization".to_owned(), format!("Bearer {token}")),
            ("Content-Type".to_owned(), "application/json".to_owned()),
        ]))
            .try_into()
            .map_err(|_| "invalid token")?;

        Ok(AirtableStore {
            http_client: reqwest::Client::builder()
                .default_headers(headers)
                .build()?,
            ratelimiter: Arc::new(Ratelimiter::new(Duration::from_secs(1), 5)),
        })
    }

    async fn request(
        &self,
        method: Method,
        url: &str,
        query: HashMap<String, String>,
        body: Option<Value>,
    ) -> Result<Value, AirtableStoreError> {
        self.ratelimiter.ask().await;

        let mut req = self.http_client.request(method, url).query(&query);

        if let Some(b) = body {
            req = req.body(serde_json::to_string(&b)?)
        }

        let val = req.send().await?.text().await?;

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

                let resp = this.request(Method::GET, &url, paged_q, None).await?;

                let bases = resp
                    .get(&object_key)
                    .ok_or(format!("No {object_key} in resp: {resp}"))?
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
    pub fn by_id(id: &str) -> Self {
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
impl<V: 'static + Serialize + DeserializeOwned + Clone + Debug + Eq> SubAddress<AirtableTable<V>>
    for AirtableBase
{
    type Output = AirtableTable<V>;

    fn sub(self, mut rhs: AirtableTable<V>) -> Self::Output {
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
    type AddedAddress = AirtableTable<Value>;

    type ItemAddress = AirtableTable<Value>;

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
                phantom: PhantomData,
            };
            Ok((b.clone(), b))
        })
        .boxed_local()
    }
}

pub struct AirtableTable<V> {
    pub id: String,
    pub base: Option<AirtableBase>,
    pub meta: Option<Value>,
    phantom: PhantomData<V>,
}

impl<V> AirtableTable<V> {
    pub fn by_id_or_name(id_or_name: &str) -> Self {
        AirtableTable {
            id: id_or_name.to_owned(),
            base: None,
            meta: None,
            phantom: PhantomData,
        }
    }
}

impl<V> Clone for AirtableTable<V> {
    fn clone(&self) -> Self {
        AirtableTable {
            id: self.id.to_owned(),
            base: self.base.clone(),
            meta: self.meta.clone(),
            phantom: PhantomData,
        }
    }
}
impl<V> PartialEq for AirtableTable<V> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl<V> Eq for AirtableTable<V> {}

impl<V> Debug for AirtableTable<V> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AirtableTable")
            .field("id", &self.id)
            .field("base", &self.base)
            .field("meta", &self.meta)
            .finish()
    }
}

impl<V: 'static> Address for AirtableTable<V> {
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
impl<V: 'static> Addressable<AirtableTable<V>> for AirtableStore {}

// TODO: id/value stuff is a bit of a boilerplate
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AirtableRecord<V: Serialize + DeserializeOwned> {
    pub id: String,
    pub table: AirtableTable<V>,
    pub value: Option<V>,
}

impl<V: 'static + Serialize + DeserializeOwned + Clone + Debug + Eq> SubAddress<AirtableRecord<V>>
    for AirtableTable<V>
{
    type Output = AirtableRecord<V>;

    fn sub(self, rhs: AirtableRecord<V>) -> Self::Output {
        assert!(self == rhs.table);

        rhs
    }
}

impl<V: 'static + Serialize + DeserializeOwned + Clone + Debug + Eq> Address for AirtableRecord<V> {
    fn own_name(&self) -> String {
        self.id.to_owned()
    }

    fn as_parts(&self) -> Vec<String> {
        let mut v = self.table.as_parts();
        v.push(self.id.to_owned());
        v
    }
}
impl<V: 'static + Serialize + DeserializeOwned + Clone + Debug + Eq> Addressable<AirtableRecord<V>>
    for AirtableStore
{
    type DefaultValue = V;
}

impl<'a, V: 'static + Serialize + DeserializeOwned + Clone + Debug + Eq>
    AddressableList<'a, AirtableTable<V>> for AirtableStore
{
    type AddedAddress = AirtableRecord<V>;

    type ItemAddress = AirtableRecord<V>;

    fn list(&self, addr: &AirtableTable<V>) -> Self::ListOfAddressesStream {
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
                        value: serde_json::from_value(value["fields"].clone())?,
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
pub struct FilterByFormula(pub String);

impl<'a, V: 'static + Serialize + DeserializeOwned + Clone + Debug + Eq>
    AddressableQuery<'a, FilterByFormula, AirtableTable<V>> for AirtableStore
{
    fn query(
        &self,
        addr: &AirtableTable<V>,
        query: FilterByFormula,
    ) -> Self::ListOfAddressesStream {
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

impl<'a, V: 'static + Serialize + DeserializeOwned + Clone + Debug + Eq>
    AddressableInsert<'a, V, AirtableTable<V>> for AirtableStore
{
    fn insert(&self, addr: &AirtableTable<V>, items: Vec<V>) -> Self::ListOfAddressesStream {
        let pages = items.chunks(10).map(|c| c.to_vec()).collect::<Vec<_>>();
        let this = self.clone();
        let addr = addr.clone();

        stream::iter(pages)
            .then(move |page| {
                let addr = addr.clone();
                let this = this.clone();

                async move {
                    let records = page
                        .iter()
                        .map(|v| {
                            let fields = serde_json::to_value(v)?;
                            Ok(json!({ "fields": fields }))
                        })
                        .collect::<Result<Vec<_>, AirtableStoreError>>()?;

                    let data = json!({ "records": records });

                    let url = format!(
                        "https://api.airtable.com/v0/{}/{}",
                        addr.base
                            .clone()
                            .ok_or(AirtableStoreError::Custom(
                                "Table address contains no base address".to_owned()
                            ))?
                            .id,
                        addr.id
                    );

                    let val = this
                        .request(Method::POST, &url, Default::default(), Some(data))
                        .await?;

                    // println!("val: {val:?}");
                    // println!("data: {}", serde_json::to_string_pretty(&data)?);

                    let records = val
                        .get("records")
                        .ok_or("no records field")?
                        .as_array()
                        .ok_or(AirtableStoreError::Custom(format!(
                            "Airtable response does not contain records: {val:?}",
                        )))?
                        .iter()
                        .map(move |v| {
                            Ok::<_, AirtableStoreError>(AirtableRecord {
                                id: v["id"]
                                    .as_str()
                                    .ok_or("Airtable record does not have an id")?
                                    .to_owned(),
                                table: addr.clone(),
                                value: Some(serde_json::from_value::<V>(v["fields"].clone())?),
                            })
                        })
                        .collect::<Vec<_>>();

                    Ok::<_, AirtableStoreError>(stream::iter(records))
                }
            })
            .try_flatten()
            .map_ok(|r| (r.clone(), r))
            .boxed_local()
    }
}

#[cfg(test)]
mod test_airtable {
    use std::collections::HashMap;

    use crate::{
        store::StoreEx,
        stores::cloud::airtable::{
            AirtableBase, AirtableBasesRootAddr, AirtableStore, AirtableTable, FilterByFormula,
        },
    };
    use futures::{StreamExt, TryStreamExt};
    use serde_json::Value;

    #[tokio::test]
    #[ignore]
    pub async fn test_airtable() -> Result<(), Box<dyn std::error::Error>> {
        let store =
            AirtableStore::new(&std::env::var("AIRTABLE_API_KEY").expect("AIRTABLE_API_KEY"))?;

        // println!("");
        // println!("");
        // println!("Will test rate-limit...");
        // // let mut bases = store.sub(AirtableBasesRootAddr).list();

        // future::join_all((0..100).map(|_| async {
        //     store
        //         .sub(AirtableBasesRootAddr)
        //         .list()
        //         .try_collect::<Vec<_>>()
        //         .await
        //         .unwrap()
        // }))
        // .await;

        println!();
        println!();
        println!("Will test hierarchy...");

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

        println!();
        println!();
        println!("Will query...");

        let mut query = store
            .sub(AirtableBase::by_id("app46Mmalo62fN5Vq"))
            .sub(AirtableTable::<Option<Value>>::by_id_or_name("Entries"))
            .query(FilterByFormula("Find(\"RPC\", {title})".to_owned()));

        while let Some(v) = query.next().await {
            let (v, _) = v?;
            println!("    {:?}", v.value.unwrap());
        }

        let loc = store
            .sub(AirtableBase::by_id("appkdGdMEeflhZSr2"))
            .sub(AirtableTable::<HashMap<String, String>>::by_id_or_name(
                "Test",
            ));

        println!();
        println!();
        println!("Will insert...");

        let res = loc
            .insert(vec![
                HashMap::from([("a".to_owned(), "b".to_owned())]),
                HashMap::from([
                    ("a".to_owned(), "b2".to_owned()),
                    ("c".to_owned(), "d2".to_owned()),
                ]),
                HashMap::from([
                    ("a".to_owned(), "u2".to_owned()),
                    ("c".to_owned(), "d2".to_owned()),
                    ("e".to_owned(), "f".to_owned()),
                ]),
            ])
            .try_collect::<Vec<_>>()
            .await?;

        println!("insert: {:?}", res);

        let res = loc.list().try_collect::<Vec<_>>().await?;

        println!("inserted: {:?}", res);

        // let obj = loc.sub(res[0].0).getv().await?;

        // println!("v: {:?}", res);

        Ok(())
        // Err(AirtableStoreError::Custom("lol".to_owned()))?
    }
}
