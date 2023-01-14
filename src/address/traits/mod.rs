use std::pin::Pin;

use futures::Stream;

use crate::store::StoreResult;

pub use super::{Address, Addressable, SubAddress};

mod list;
mod tree;

pub use list::*;
pub use tree::*;

pub trait AddressableGet<Value, A: Address>: Addressable<A> {
    async fn addr_get(&self, addr: &A) -> StoreResult<Option<Value>, Self>;
}

pub trait AddressableSet<Value, A: Address>: Addressable<A> {
    async fn set_addr(&self, addr: &A, value: &Option<Value>) -> StoreResult<(), Self>;
}
