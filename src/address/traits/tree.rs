use derive_more::Display;

use crate::store::StoreResult;

use super::{AddressableList, SubAddress};

#[derive(Debug, Clone, Display, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum BranchOrLeaf<B, L> {
    Branch(B),
    Leaf(L),
}

impl<B, L> BranchOrLeaf<B, L> {
    pub fn unit(&self) -> BranchOrLeaf<(), ()> {
        match self {
            BranchOrLeaf::Branch(_) => BranchOrLeaf::Branch(()),
            BranchOrLeaf::Leaf(_) => BranchOrLeaf::Leaf(()),
        }
    }
}

pub trait AddressableTree<'a, TreeAddr, ItemAddr>:
    AddressableList<'a, TreeAddr, ItemAddress = TreeAddr>
where
    TreeAddr: SubAddress<Self::AddedAddress, Output = TreeAddr>,
{
    async fn branch_or_leaf(
        &self,
        addr: TreeAddr,
    ) -> StoreResult<BranchOrLeaf<TreeAddr, ItemAddr>, Self>;
}

#[cfg(test)]
#[cfg(feature = "json")]
mod test {
    use std::collections::HashSet;

    use futures::{StreamExt, TryStreamExt};
    use serde_json::json;

    use crate::{
        store::*,
        stores::json::{paths::JsonPath, *},
        wrappers::filter_addresses::{FilterAddressesWrapperError, FilterAddressesWrapperStore},
    };

    #[tokio::test]
    async fn test() -> Result<(), anyhow::Error> {
        let val = json!({
            "wow": {"hello": "yes"},
            "another": {"seriously": {"throrougly": 7}, "basic": [1, 2, 3, {"hello": "_why"}, {"_why": "ya"}]},
            "_ignore": {"haha": {"_yes": 3}}
        });
        let store = FilterAddressesWrapperStore::new(json_value_store(val)?, |s: JsonPath| {
            s.last()
                .map(|s| !s.to_key().starts_with('_'))
                .unwrap_or(true)
        });
        let root = store.root();

        let all_paths = root
            .walk_tree_recursively()
            .map_ok(|v| v.to_string())
            .try_collect::<HashSet<_>>()
            .await?;

        println!("{all_paths:?}");

        assert!(all_paths.contains("another.basic[2]"));
        assert!(all_paths.contains("another"));
        assert!(!all_paths.contains("_ignore"));
        assert!(!all_paths.contains("_ignore.haha"));
        assert!(!all_paths.contains("another.basic[4]._why"));
        assert!(all_paths.contains("wow.hello"));
        assert!(!all_paths.contains("wow.hello.nonexistent"));
        assert!(all_paths.contains("another.basic[3].hello"));

        Ok(())
    }
}
