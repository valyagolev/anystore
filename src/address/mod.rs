use std::fmt::Debug;

use crate::store::Store;

pub mod primitive;
pub mod traits;

/// Must be a syntactically valid address: successfully parsed, but not yet validated.
///
/// You will want to implement [`SubAddress<NextPart>`](SubAddress) for this if your store supports several layers of indirection,
/// as `location.sub(addr_part: NextPart)` will add them.
///
/// `Value`: Typically it should be `Option<SomeValue>`, to indicate when the item is not found,
/// or deletion of the item when it's written.
// pub trait Address<S: Store + ?Sized>: Address {
//     type DefaultValue = !;

//     // fn own_name(&self) -> String;
// }

pub trait Address: Eq + Clone + Debug + 'static {
    /// This should be an addressable, unique id in the container (not "own name")
    fn own_name(&self) -> String;

    /// The whole path
    fn as_parts(&self) -> Vec<String>;
}

pub trait Addressable<A: Address>: Store {
    type DefaultValue = !;
}

pub trait PathAddress: Address {
    type Error;
    type Output;

    fn path(self, str: &str) -> Result<Self::Output, Self::Error>;
}

pub trait SubAddress<Sub>: Address {
    type Output: Address;

    fn sub(self, sub: Sub) -> Self::Output;
}
