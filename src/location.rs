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
            tree::{AddressableTree, BranchOrLeaf},
            AddressableList, AddressableRead, AddressableWrite,
        },
        Address, Addressable, PathAddress, SubAddress,
    },
    store::{Store, StoreEx, StoreResult},
};
use futures::StreamExt;
use futures::{stream, Stream};

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Location<Addr: Address, S: Store + Addressable<Addr>> {
    store: S,
    pub address: Addr,
}

impl<'a, Addr: Address, S: 'a + Store + Addressable<Addr>> Location<Addr, S> {
    pub fn new(address: Addr, store: S) -> Self {
        Location { store, address }
    }

    pub fn sub<AR: Address, A2>(self, address: A2) -> Location<AR, S>
    where
        Addr: SubAddress<A2, Output = AR>,
        S: Addressable<AR>,
    {
        Location::new(self.address.sub(address), self.store)
    }

    pub fn path<A: Address>(self, p: &str) -> StoreResult<Location<A, S>, S>
    where
        S: Addressable<A>,
        Addr: PathAddress<Output = A>,
        <S as Store>::Error: From<<Addr as PathAddress>::Error>,
    {
        Ok(Location::new(self.address.path(p)?, self.store))
    }

    pub async fn get<Value>(&self) -> StoreResult<Option<Value>, S>
    where
        S: AddressableRead<Value, Addr>,
    {
        self.store.read(&self.address).await
    }

    pub async fn write<Value>(&self, value: &Option<Value>) -> StoreResult<(), S>
    where
        S: AddressableWrite<Value, Addr>,
    {
        self.store.write(&self.address, value).await
    }

    pub fn list(&self) -> S::ListOfAddressesStream
    where
        Addr: SubAddress<S::AddedAddress, Output = S::ItemAddress>,
        S: AddressableList<'a, Addr>,
        // Addr: SubAddress<S::AddedAddress, Output = S::ItemAddress>,
    {
        self.store.list(&self.address)
    }
}

impl<Addr: Address, S: Store + AddressableRead<Existence, Addr>> Location<Addr, S> {
    pub async fn exists(&self) -> StoreResult<bool, S> {
        return Ok(self.get::<Existence>().await?.is_some());
    }
}

impl<V, Addr: Address, S: Store + Addressable<Addr, DefaultValue = V>> Location<Addr, S> {
    pub async fn getv(&self) -> StoreResult<Option<V>, S>
    where
        S: AddressableRead<V, Addr>,
    {
        self.get().await
    }
    pub async fn writev(&self, v: &Option<V>) -> StoreResult<(), S>
    where
        S: AddressableWrite<V, Addr>,
    {
        self.write(v).await
    }
}

impl<'a, ListAddr: Address, S: 'a + Store + Addressable<ListAddr>> Location<ListAddr, S> {
    #![cfg_attr(not(feature = "json"), doc = "```ignore")]
    #![cfg_attr(feature = "json", doc = "```")]
    /// use std::collections::HashSet;
    /// use futures::TryStreamExt;
    /// use serde_json::json;
    ///
    /// use anystore::stores::json::*;
    /// use anystore::wrappers::tree::ignore_keys::*;
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
    /// let store = FilterAddressesWrapperStore::new(JsonValueStore::new(val), |s: JsonPath| {
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
