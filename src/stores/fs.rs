// #[derive(Debug, Clone)]
// pub struct FileSystemLocation<const IsFile: FileOrDir> {
//     root: Arc<PathBuf>,
//     path: RelativePath,
// }

use std::{ffi::OsString, path::PathBuf, string::FromUtf8Error, sync::Arc};

use derive_more::{Display, From};
use futures::{stream, FutureExt, StreamExt, TryStreamExt};
use thiserror::Error;
use tokio::fs::DirEntry;

use crate::{
    address::{
        primitive::Existence,
        traits::{
            tree::{AddressableTree, BranchOrLeaf},
            AddressableList, AddressableRead, AddressableWrite,
        },
        Address, Addressable, PathAddress, SubAddress,
    },
    store::{Store, StoreResult},
};

#[derive(Error, Display, Debug, From)]
pub enum FileStoreError {
    SomeError(String),
    StdIoError(std::io::Error),
    FromUtf8Error(FromUtf8Error),

    #[from(ignore)]
    UnsupportedFeature(String),
}

#[derive(PartialEq, Eq, Debug, Clone, From)]
pub struct RelativePath(PathBuf);

#[derive(PartialEq, Eq, Debug, Clone, From, Display)]
pub struct FilePath(RelativePath);

impl std::fmt::Display for RelativePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

impl From<&str> for RelativePath {
    fn from(value: &str) -> Self {
        RelativePath(value.into())
    }
}
impl From<String> for RelativePath {
    fn from(value: String) -> Self {
        RelativePath(value.into())
    }
}
impl From<OsString> for RelativePath {
    fn from(value: OsString) -> Self {
        RelativePath(value.into())
    }
}

impl From<RelativePath> for String {
    fn from(value: RelativePath) -> Self {
        value.to_string()
    }
}

impl From<FilePath> for String {
    fn from(value: FilePath) -> Self {
        value.to_string()
    }
}

impl From<crate::address::primitive::UniqueRootAddress> for RelativePath {
    fn from(_value: crate::address::primitive::UniqueRootAddress) -> Self {
        "".into()
    }
}

impl PathAddress for RelativePath {
    type Error = FileStoreError;

    type Output = RelativePath;

    fn path(self, str: &str) -> Result<Self::Output, Self::Error> {
        // todo: validation?
        Ok(Self(self.0.join(str)))
    }
}

impl Address for RelativePath {
    fn own_name(&self) -> String {
        self.0
            .components()
            .last()
            .map(|p| {
                p.as_os_str()
                    .to_str()
                    .expect("Non-unicode is not supported")
            })
            .unwrap_or("")
            .to_owned()
    }

    fn as_parts(&self) -> Vec<String> {
        todo!()
    }
}

impl SubAddress<RelativePath> for RelativePath {
    type Output = RelativePath;

    fn sub(self, sub: RelativePath) -> Self::Output {
        Self(self.0.join(sub.0))
    }
}

#[derive(Debug, Clone)]
pub struct FileSystemStore {
    base_directory: Arc<PathBuf>,
}

impl FileSystemStore {
    pub fn new(path: PathBuf) -> Self {
        FileSystemStore {
            base_directory: Arc::new(path),
        }
    }

    pub fn here() -> StoreResult<Self, Self> {
        Ok(Self::new(std::env::current_dir()?))
    }

    pub fn get_complete_path(&self, addr: RelativePath) -> PathBuf {
        self.base_directory.join(addr.0)
    }
}

impl Store for FileSystemStore {
    type Error = FileStoreError;

    type RootAddress = RelativePath;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOrDir {
    File(String),
    Dir,
}

impl Addressable<RelativePath> for FileSystemStore {
    type DefaultValue = FileOrDir;
}

impl AddressableRead<String, RelativePath> for FileSystemStore {
    async fn read(&self, addr: &RelativePath) -> StoreResult<Option<String>, Self> {
        match tokio::fs::read(self.get_complete_path(addr.clone())).await {
            Ok(fil) => Ok(Some(String::from_utf8(fil)?)),
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => Ok(None),
                _ => Err(e.into()),
            },
        }
    }
}

impl AddressableWrite<String, RelativePath> for FileSystemStore {
    async fn write(&self, addr: &RelativePath, value: &Option<String>) -> StoreResult<(), Self> {
        let path = self.get_complete_path(addr.clone());

        // todo: create dirs?

        match value {
            None => todo!("deletion"),
            Some(contents) => Ok(tokio::fs::write(path, contents).await?),
        }
    }
}

impl AddressableRead<Existence, RelativePath> for FileSystemStore {
    async fn read(&self, addr: &RelativePath) -> StoreResult<Option<Existence>, Self> {
        let m = tokio::fs::metadata(self.get_complete_path(addr.clone())).await;

        match m {
            Ok(_) => Ok(Some(Existence)),
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => Ok(None),
                _ => Err(e.into()),
            },
        }
    }
}

impl<'a> AddressableList<'a, RelativePath> for FileSystemStore {
    type AddedAddress = RelativePath;

    type ItemAddress = RelativePath;

    type ListOfAddressesStream = std::pin::Pin<
        Box<
            dyn 'a
                + futures::Stream<Item = StoreResult<(Self::AddedAddress, Self::ItemAddress), Self>>,
        >,
    >;

    fn list(&self, addr: &RelativePath) -> Self::ListOfAddressesStream {
        let this = self.clone();
        let addr = addr.clone();
        let addr2 = addr.clone();

        stream::once(async move {
            let stream = tokio_stream::wrappers::ReadDirStream::new(
                tokio::fs::read_dir(this.get_complete_path(addr.clone())).await?,
            )
            .map_err(|e| e.into());

            Ok::<_, FileStoreError>(stream)
        })
        .try_flatten()
        .and_then(move |de: DirEntry| {
            let addr = addr2.clone();

            async move {
                let name = de.file_name();

                Ok((name.clone().into(), addr.sub(name.into())))
            }
        })
        .boxed_local()
    }
}

impl<'a> AddressableTree<'a, RelativePath, FilePath> for FileSystemStore {
    async fn branch_or_leaf(
        &self,
        addr: RelativePath,
    ) -> StoreResult<BranchOrLeaf<RelativePath, FilePath>, Self> {
        let typ = tokio::fs::metadata(self.get_complete_path(addr.clone()))
            .await?
            .file_type();

        if typ.is_dir() {
            Ok(BranchOrLeaf::Branch(addr))
        } else if typ.is_file() {
            Ok(BranchOrLeaf::Leaf(addr.into()))
        } else {
            Err(FileStoreError::UnsupportedFeature(format!(
                "Neither file nor dir: {typ:?}"
            )))
        }
    }
}

impl Address for FilePath {
    fn own_name(&self) -> String {
        self.0.own_name()
    }

    fn as_parts(&self) -> Vec<String> {
        self.0.as_parts()
    }
}
