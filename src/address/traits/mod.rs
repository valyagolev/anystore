use std::pin::Pin;

use futures::Stream;

use crate::store::StoreResult;

pub use super::{Address, Addressable, SubAddress};

mod list;
mod tree;

pub use list::*;
pub use tree::*;

pub trait AddressableRead<Value, A: Address>: Addressable<A> {
    async fn read(&self, addr: &A) -> StoreResult<Option<Value>, Self>;
}

pub trait AddressableWrite<Value, A: Address>: Addressable<A> {
    async fn write(&self, addr: &A, value: &Option<Value>) -> StoreResult<(), Self>;
}

pub trait AddressableInsert<Value, A: Address>:
    Addressable<A> + Addressable<Self::ItemAddress>
{
    type ItemAddress: Address;

    async fn insert(&self, addr: &A, value: &Value) -> StoreResult<Self::ItemAddress, Self>;
}
