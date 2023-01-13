use std::fmt::Display;

use super::{Address, SubAddress};

/// Default value for layers that can't be read/written as values (e.g. in some cases the root layer)
pub type OpaqueValue = !;

/// Simply a unit type for the root address. `store.root()` returns the location for this one.
///
/// It implements `SubAddress<A>` for every type, simply returning the other operand,
/// so you can easily chain them.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct UniqueRootAddress;

impl Display for UniqueRootAddress {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

impl Address for UniqueRootAddress {
    fn own_name(&self) -> String {
        "".to_owned()
    }

    fn as_parts(&self) -> Vec<String> {
        vec![]
    }
}

impl<A: Address> SubAddress<A> for UniqueRootAddress {
    type Output = A;

    fn sub(self, rhs: A) -> Self::Output {
        rhs
    }
}

/// Ask for this if you only care about the existence of a key
///
/// Implement `AddressFor<Existence, S>` if you know how to
/// ask for existence
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub struct Existence;

// #[derive(PartialEq, Eq, Clone, Default, Debug)]
// pub struct ListOfAddresses<BaseAddr: Address + SubAddress<SubAddr>, SubAddr: Clone> {
//     pub base: BaseAddr,
//     pub list: Vec<SubAddr>,
// }

// impl<
//         BaseAddr: Address + SubAddress<SubAddr, Output = WholeAddress>,
//         SubAddr: Clone,
//         WholeAddress,
//         // S: Store + Addressable<BaseAddr> + Addressable<WholeAddress>,
//     > ListOfAddresses<BaseAddr, SubAddr>
// {
//     pub fn new(base: BaseAddr, list: Vec<SubAddr>) -> Self {
//         ListOfAddresses { base, list }
//     }
// }

// impl<
//         BaseAddr: Address + SubAddress<SubAddr, Output = WholeAddress>,
//         SubAddr: Clone,
//         WholeAddress,
//     > IntoIterator for ListOfAddresses<BaseAddr, SubAddr>
// {
//     type Item = WholeAddress;

//     type IntoIter = std::vec::IntoIter<WholeAddress>;

//     fn into_iter(self) -> Self::IntoIter {
//         let base = self.base.clone();

//         self.list
//             .into_iter()
//             .map(|sub: SubAddress| base.clone().sub(sub))
//             .collect::<Vec<_>>()
//             .into_iter()
//     }
// }
