// trait LocationFor<Value, Address: AddressFor<Value, S>, S> {}

// use crate::{
//     address::{primitive::Existence, AddressFor},
//     store::{list::ListOfAddresses, ReadStore, Store, StoreResult},
// };

use std::pin::Pin;

use crate::{
    address::{
        primitive::Existence,
        traits::{
            AddressableInsert, AddressableList, AddressableQuery, AddressableRead, AddressableTree,
            AddressableWrite, BranchOrLeaf,
        },
        Address, Addressable, PathAddress, SubAddress,
    },
    store::{Store, StoreEx, StoreResult},
};
use futures::StreamExt;
use futures::{stream, Stream};

/// A pair of a store and an address. You can pass this object around,
/// use it to traverse the store, and get/change values.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Location<Addr: Address, S: Store + Addressable<Addr>> {
    pub store: S,
    pub address: Addr,
}

impl<V, Addr: Address, S: Store + Addressable<Addr, DefaultValue = V>> Location<Addr, S> {
    /// Get a Value of the default type for this address.
    pub async fn getv(&self) -> StoreResult<Option<V>, S>
    where
        S: Addressable<Addr, DefaultValue = V> + AddressableRead<V, Addr>,
    {
        self.get().await
    }

    /// Write a Value of the default type for this address.
    pub async fn writev(&self, v: &Option<V>) -> StoreResult<(), S>
    where
        S: Addressable<Addr, DefaultValue = V> + AddressableWrite<V, Addr>,
    {
        self.write(v).await
    }
}

impl<'a, Addr: Address, S: 'a + Store + Addressable<Addr>> Location<Addr, S> {
    /// Stream sub-addresses from this location.
    ///
    /// It try-streams pairs of `(sub, full_address)`, where `sub` is the part
    /// of the address added to this one to get `full_address`
    /// (like, filename vs full path to the file).
    pub fn list(&self) -> S::ListOfAddressesStream
    where
        Addr: SubAddress<S::AddedAddress, Output = S::ItemAddress>,
        S: AddressableList<'a, Addr>,
    {
        self.store.list(&self.address)
    }

    /// Type-safe navigation. Every store defines its own address types.
    ///
    #[cfg_attr(not(feature = "json"), doc = "```ignore")]
    #[cfg_attr(feature = "json", doc = "```no_run")]
    /// # use anystore::stores::json::*;
    /// # use anystore::location::Location;
    /// # fn testloc(jsonlocation: Location<JsonPath, JsonValueStore>) {
    /// let location = jsonlocation
    ///                     .sub(JsonPathPart::Key("subkey".to_owned()))
    ///                     .sub(JsonPathPart::Index(2));
    /// # }
    /// ```
    pub fn sub<AR: Address, A2>(self, address: A2) -> Location<AR, S>
    where
        Addr: SubAddress<A2, Output = AR>,
        S: Addressable<AR>,
    {
        Location::new(self.address.sub(address), self.store)
    }

    /// String-based navigation. Some stores allow this.
    ///
    #[cfg_attr(not(feature = "json"), doc = "```ignore")]
    #[cfg_attr(feature = "json", doc = "```no_run")]
    /// # use anystore::stores::json::*;
    /// # use anystore::store::StoreResult;
    /// # use anystore::location::Location;
    /// # fn testloc(jsonlocation: Location<JsonPath, JsonValueStore>) -> StoreResult<(), JsonValueStore> {
    /// let location = jsonlocation.path("subkey[2]")?.path("deeper.anotherone[12]")?;
    /// # Ok(()) };
    /// ```
    pub fn path<A: Address>(self, p: &str) -> StoreResult<Location<A, S>, S>
    where
        S: Addressable<A>,
        Addr: PathAddress<Output = A>,
        <S as Store>::Error: From<<Addr as PathAddress>::Error>,
    {
        Ok(Location::new(self.address.path(p)?, self.store))
    }

    /// Get a Value of a parituclar type from the store, if the store supports that.
    ///
    /// Often it's easier to use `location.getv()`, as it will return the default type
    /// for this kind of location.
    ///
    /// `None` means that the value doesn't exist.
    pub async fn get<Value>(&self) -> StoreResult<Option<Value>, S>
    where
        S: AddressableRead<Value, Addr>,
    {
        self.store.read(&self.address).await
    }

    /// Write a Value of a particular type to the store, if the store supports that.
    ///
    /// Often it's easier to use `location.writev(value)`, as it will use the default type
    /// for this kind of location.
    ///
    /// `None` means that the value doesn't exist.
    pub async fn write<Value>(&self, value: &Option<Value>) -> StoreResult<(), S>
    where
        S: AddressableWrite<Value, Addr>,
    {
        self.store.write(&self.address, value).await
    }

    pub fn insert<Value>(&self, values: Vec<Value>) -> S::ListOfAddressesStream
    where
        S: AddressableInsert<'a, Value, Addr>,
        Addr: SubAddress<S::AddedAddress, Output = S::ItemAddress>,
    {
        self.store.insert(&self.address, values)
    }

    pub fn query<Query>(&self, query: Query) -> S::ListOfAddressesStream
    where
        Addr: SubAddress<S::AddedAddress, Output = S::ItemAddress>,
        S: AddressableQuery<'a, Query, Addr>,
    {
        self.store.query(&self.address, query)
    }

    /// Typically it's better to use `store.sub(address)`
    pub fn new(address: Addr, store: S) -> Self {
        Location { store, address }
    }
}

impl<Addr: Address, S: Store + AddressableRead<Existence, Addr>> Location<Addr, S> {
    /// Check existence by the address.
    pub async fn exists(&self) -> StoreResult<bool, S> {
        return Ok(self.get::<Existence>().await?.is_some());
    }
}

impl<'a, ListAddr: Address, S: 'a + Store + Addressable<ListAddr>> Location<ListAddr, S> {
    /// Recursively traverse the tree and stream all the addresses.
    ///
    #[cfg_attr(not(feature = "json"), doc = "```ignore")]
    #[cfg_attr(feature = "json", doc = "```")]
    /// use std::collections::HashSet;
    /// use futures::TryStreamExt;
    /// use serde_json::json;
    ///
    /// use anystore::stores::json::*;
    /// use anystore::wrappers::filter_addresses::*;
    /// use anystore::store::StoreEx;
    ///
    /// # tokio_test::block_on(async {
    ///
    /// let val = json!({
    ///     "wow": {"hello": "yes"},
    ///     "another": {"seriously": {"throrougly": 7}, "basic": [1, 2, 3, {"hello": "_why"}, {"_why": "ya"}]},
    ///     "_ignore": {"haha": {"_yes": 3}}
    /// });
    ///
    /// let store = FilterAddressesWrapperStore::new(json_value_store(val)?, |s: JsonPath| {
    ///     s.last()
    ///         .map(|s| !s.to_key().starts_with("_"))
    ///         .unwrap_or(true)
    /// });
    /// let root = store.root();
    ///
    /// let all_paths = root
    ///     .walk_tree_recursively()
    ///     .map_ok(|v| v.to_string())
    ///     .try_collect::<HashSet<_>>()
    ///     .await?;
    ///
    /// println!("{:?}", all_paths);
    ///
    ///
    /// assert!(all_paths.contains("another.basic[2]"));
    /// assert!(all_paths.contains("another"));
    /// assert!(all_paths.contains("another.basic[3].hello"));
    /// assert!(all_paths.contains("wow.hello"));
    /// assert!(!all_paths.contains("wow.hello.nonexistent"));
    /// assert!(!all_paths.contains("_ignore"));
    /// assert!(!all_paths.contains("_ignore.haha"));
    /// assert!(!all_paths.contains("another.basic[4]._why"));
    ///
    ///
    /// Ok::<(), FilterAddressesWrapperError<JsonValueStoreError>>(())
    ///
    /// # }).unwrap()
    /// ```
    pub fn walk_tree_recursively<ItemAddr>(
        &self,
    ) -> impl 'a + Stream<Item = StoreResult<BranchOrLeaf<ListAddr, ItemAddr>, S>>
    where
        ItemAddr: Address,
        S: AddressableTree<'a, ListAddr, ItemAddr>,
        S::AddedAddress: std::fmt::Debug,
        ListAddr: SubAddress<S::AddedAddress, Output = ListAddr>,
    {
        let store = self.store.clone();
        let to_visit: Vec<Pin<Box<S::ListOfAddressesStream>>> = vec![Box::pin(self.list())];

        stream::try_unfold(to_visit, move |mut to_visit| {
            let store = store.clone();

            async move {
                while let Some(last) = to_visit.last_mut() {
                    let Some(val) = last.next().await else {
                        to_visit.pop();
                        continue;
                    };

                    let (_, val) = val?;
                    let bl = store.branch_or_leaf(val).await?;

                    match bl {
                        BranchOrLeaf::Leaf(_) => {
                            return Ok(Some((bl, to_visit)));
                        }
                        BranchOrLeaf::Branch(br) => {
                            to_visit.push(Box::pin(store.sub(br.clone()).list()));

                            return Ok(Some((BranchOrLeaf::Branch(br), to_visit)));
                        }
                    }

                    //
                }

                Ok(None)
            }
        })
    }
}
