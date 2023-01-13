use std::pin::Pin;

use futures::Stream;

use crate::store::StoreResult;

pub use super::{Address, Addressable, SubAddress};

pub mod tree;

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

pub trait AddressableList<
    'a,
    ListAddr: Address + SubAddress<Self::AddedAddress, Output = Self::ItemAddress>,
>: Addressable<ListAddr> + Addressable<Self::ItemAddress>
{
    /// The "added" part of the address (filename in the dir, key of the map, etc.)
    type AddedAddress: Clone + 'static;
    /// The address of the item in the list.
    type ItemAddress: Address;

    /// Uses pinned stream as a reasonable default because most of the
    /// time you probably don't care too much.
    /// You can use `.boxed_local()` on any stream with the correct items
    /// to create this type.
    type ListOfAddressesStream: 'a
        + Stream<Item = StoreResult<(Self::AddedAddress, Self::ItemAddress), Self>> = Pin<
        Box<dyn 'a + Stream<Item = StoreResult<(Self::AddedAddress, Self::ItemAddress), Self>>>,
    >;

    fn list(&self, addr: &ListAddr) -> Self::ListOfAddressesStream;
}
