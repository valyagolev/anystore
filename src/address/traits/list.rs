use super::*;

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

pub trait AddressableInsert<
    'a,
    Value,
    ListAddr: Address + SubAddress<Self::AddedAddress, Output = Self::ItemAddress>,
>: AddressableList<'a, ListAddr>
{
    /// Inserts a list, returning the addresses of the items.
    fn insert(&self, addr: &ListAddr, items: Vec<Value>) -> Self::ListOfAddressesStream;
}

pub trait AddressableQuery<
    'a,
    Query,
    ListAddr: Address + SubAddress<Self::AddedAddress, Output = Self::ItemAddress>,
>: AddressableList<'a, ListAddr>
{
    /// Queries a list. Every store can define its own queries.
    ///
    /// Typically this trait should be implemented only if this is somehow
    /// optimized. If this is going to perform a full listing with a client-side filter,
    /// it's better not to implement this trait, but use wrappers.
    fn query(&self, addr: &ListAddr, query: Query) -> Self::ListOfAddressesStream;
}
