// use std::{pin::Pin, sync::Arc};

// use futures::{FutureExt, Stream, StreamExt, TryStreamExt};

// use crate::traits::{
//     list::ListLocation,
//     store::{Location, Store},
//     tree::{BranchLocation, BranchOrLeaf, PathAddress, SubTreeLocation, TreeLocation},
// };

use std::{marker::PhantomData, sync::Arc};

use derive_more::Display;
use futures::{StreamExt, TryStreamExt};
use thiserror::Error;

use crate::{
    address::{
        traits::{
            tree::{AddressableTree, BranchOrLeaf},
            AddressableList, AddressableRead, AddressableWrite,
        },
        Address, Addressable, SubAddress,
    },
    store::{Store, StoreResult},
};

#[derive(Display, Debug, Error)]
pub enum FilterAddressesWrapperError<E> {
    StoreError(E),
    WriteToIgnoredLocation(String),
    SomeError(String),
}

impl<E> From<E> for FilterAddressesWrapperError<E> {
    fn from(value: E) -> Self {
        Self::StoreError(value)
    }
}

pub struct FilterAddressesWrapperStore<S: Store, K: Clone, F: Fn(K) -> bool> {
    underlying: S,
    filter: Arc<F>,
    phantom_key: PhantomData<K>,
}

impl<S: Store, K: Clone, F: Fn(K) -> bool> Clone for FilterAddressesWrapperStore<S, K, F> {
    fn clone(&self) -> Self {
        Self {
            underlying: self.underlying.clone(),
            filter: self.filter.clone(),
            phantom_key: self.phantom_key,
        }
    }
}

impl<S: Store, K: Clone, F: Fn(K) -> bool> FilterAddressesWrapperStore<S, K, F>
where
    S::RootAddress: Into<K>,
{
    pub fn new(underlying: S, filter: F) -> Self {
        FilterAddressesWrapperStore {
            underlying,
            filter: Arc::new(filter),
            phantom_key: PhantomData,
        }
    }

    pub fn destruct(self) -> S {
        self.underlying
    }

    pub fn should_ignore_addr<Addr: Address + Into<K>>(&self, addr: &Addr) -> bool {
        // todo: avoid this cloning by using lots of refs?
        !(self.filter)(addr.clone().into())
    }

    pub fn check_ignore_addr<Addr: Address + Into<K>>(&self, addr: &Addr) -> StoreResult<(), Self> {
        if self.should_ignore_addr(addr) {
            Err(FilterAddressesWrapperError::WriteToIgnoredLocation(
                format!("{addr:?}"),
            ))
        } else {
            Ok(())
        }
    }
}

impl<S: Store, K: Clone, F: Fn(K) -> bool> Store for FilterAddressesWrapperStore<S, K, F>
where
    S::RootAddress: Into<K>,
{
    type Error = FilterAddressesWrapperError<S::Error>;

    type RootAddress = S::RootAddress;
}
impl<A: Address, S: Addressable<A>, K: Clone, F: Fn(K) -> bool> Addressable<A>
    for FilterAddressesWrapperStore<S, K, F>
where
    S::RootAddress: Into<K>,
{
    type DefaultValue = S::DefaultValue;
}
impl<V, A: Address, S: AddressableRead<V, A>, K: Clone, F: Fn(K) -> bool> AddressableRead<V, A>
    for FilterAddressesWrapperStore<S, K, F>
where
    S::RootAddress: Into<K>,
    A: Into<K>,
{
    async fn read(&self, addr: &A) -> StoreResult<Option<V>, Self> {
        if self.should_ignore_addr(addr) {
            Ok(None)
        } else {
            Ok(self.underlying.read(addr).await?)
        }
    }
}
impl<V, A: Address, S: AddressableWrite<V, A>, K: Clone, F: Fn(K) -> bool> AddressableWrite<V, A>
    for FilterAddressesWrapperStore<S, K, F>
where
    S::RootAddress: Into<K>,
    A: Into<K>,
{
    async fn write(&self, addr: &A, value: &Option<V>) -> StoreResult<(), Self> {
        self.check_ignore_addr(addr)?;

        Ok(self.underlying.write(addr, value).await?)
    }
}

impl<
        'a,
        Whole: Address,
        A: Address + SubAddress<<S as AddressableList<'a, A>>::AddedAddress, Output = Whole>,
        // TODO: is this 'static needed/fine?
        S: AddressableList<'a, A, ItemAddress = Whole> + 'a,
        K: 'a + Clone,
        F: 'a + Fn(K) -> bool,
    > AddressableList<'a, A> for FilterAddressesWrapperStore<S, K, F>
where
    S::RootAddress: Into<K>,
    A: Into<K>,
    Whole: Into<K>,
{
    type AddedAddress = S::AddedAddress;

    type ItemAddress = S::ItemAddress;

    fn list(&self, addr: &A) -> Self::ListOfAddressesStream {
        let this = self.clone();
        let addr = addr.clone();

        this.underlying
            .list(&addr)
            .filter(move |s| {
                // let s = s.clone();
                let r = match s {
                    Ok((_, whole)) => !this.should_ignore_addr(whole),
                    Err(_) => true,
                };

                async move { r }
            })
            .map_err(|e| e.into())
            .boxed_local()
    }
}

impl<
        'a,
        LA: SubAddress<S::AddedAddress, Output = LA> + Into<K>,
        IA: Into<K>,
        S: 'a + Store + AddressableTree<'a, LA, IA>,
        K: 'a + Clone + From<S::RootAddress>,
        F: 'a + Fn(K) -> bool,
    > AddressableTree<'a, LA, IA> for FilterAddressesWrapperStore<S, K, F>
{
    async fn branch_or_leaf(&self, addr: LA) -> StoreResult<BranchOrLeaf<LA, IA>, Self> {
        Ok(self.underlying.branch_or_leaf(addr).await?)
    }
}
