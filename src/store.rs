use crate::{
    address::{primitive::UniqueRootAddress, *},
    location::Location,
};

// pub mod list;
// pub mod tree;

/// Main store driver
///
/// This and the related traits are what you need to implement
/// if you want to add a store.
pub trait Store: Clone {
    type Error: std::fmt::Debug + std::fmt::Display + Send + Sync + 'static;
    type RootAddress: Address + From<UniqueRootAddress> = UniqueRootAddress;
}

pub trait StoreEx<Root: Address + From<UniqueRootAddress>>: Store {
    fn sub<Addr: Address>(&self, addr: Addr) -> Location<Addr, Self>
    where
        Self: Addressable<Addr>,
    {
        Location::new(addr, self.clone())
    }

    fn root(&self) -> Location<Root, Self>
    where
        Self: Addressable<Root>,
    {
        Location::new(UniqueRootAddress.into(), self.clone())
    }

    fn path<Addr: Address>(&self, p: &str) -> StoreResult<Location<Addr, Self>, Self>
    where
        Self: Addressable<Root> + Addressable<Addr>,
        Root: PathAddress<Output = Addr>,
        <Self as Store>::Error: From<<Root as PathAddress>::Error>, // Root::Error: From<Self::Error> + Into<Self::Error>,
    {
        self.root().path(p)
    }
}

impl<S: Store> StoreEx<S::RootAddress> for S {}

pub type StoreResult<V, S> = Result<V, <S as Store>::Error>;

// pub struct SharedStore<S: Store> {
//     store: Arc<Mutex<S>>,
// }

// impl<S: Store> S {}
// impl<S: Store> SharedStore<S> {
//     pub fn new(store: S) -> Self {
//         SharedStore {
//             store: Arc::new(Mutex::new(store)),
//         }
//     }

//     pub fn root(&self) -> Location<OpaqueValue, UniqueRootAddress, S> {
//         Location::new(UniqueRootAddress, self.store.clone())
//     }
//     pub fn sub<A: AddressFor<V, S>, V>(&self, addr: A) -> Location<V, A, S> {
//         self.root().sub(addr)
//     }
// }

// pub trait ReadStore<A: AddressFor<V, Self>, V>: Store {
//     async fn _read(&self, addr: A) -> StoreResult<Option<V>, Self>;
// }
// pub trait WriteStore<A: AddressFor<V, Self>, V>: Store {
//     async fn _write(&self, value: Option<V>, addr: A) -> StoreResult<(), Self>;
// }
