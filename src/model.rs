use chrono::prelude::*;
use digest::Digest;
use hex;
use hex::FromHex;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use std::ffi::OsString;
use std::fmt;
use std::fmt::Debug;
use std::fmt::Display;
use std::fs;
use std::os::unix::fs::MetadataExt;
use util::serde_compact_osstr;
// TODO: generic hash fn
#[derive(Clone, PartialEq, Eq, Hash)]

pub struct GittyHash {
    // #[serde(serialize_with = "buffer_to_hex")]
    pub sha256: [u8; 32],
}

/* pub trait GittyHasher {
    fn get_hasher() -> Digest<BlockSize=Any, OutputSize=Any>;
    fn convert_output(dig: impl Digest) -> GittyHash;
}*/
pub fn get_hasher() -> impl Digest {
    use sha2::Sha256;
    return Sha256::default();
}
pub fn hasher_output(dig: impl Digest) -> GittyHash {
    let mut sha256 = [0; 32];
    sha256.copy_from_slice(&dig.result()[0..32]);
    GittyHash { sha256 }
}

impl Serialize for GittyHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("sha256:{}", &hex::encode(&self.sha256)))
    }
}
impl<'de> Deserialize<'de> for GittyHash {
    fn deserialize<D>(deserializer: D) -> Result<GittyHash, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        String::deserialize(deserializer).and_then(|string| {
            let mut string = string.clone();
            let hash = string.split_off(7);
            if string != "sha256:" {
                return Err(Error::custom("not a sha256 hash"));
            }
            let v = Vec::from_hex(&hash).map_err(|err| Error::custom(err.to_string()))?;
            if v.len() != 32 {
                return Err(Error::custom("invalid sha256 hash"));
            }
            let mut sha256 = [0; 32];
            sha256.copy_from_slice(&v[0..32]);
            Ok(GittyHash { sha256 })
        })
    }
}

impl fmt::Display for GittyHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "sha256:{}", hex::encode(self.sha256))
    }
}
impl fmt::Debug for GittyHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "sha256:{}", hex::encode(self.sha256))
    }
}

pub const PLACEHOLDER_HASH: GittyHash = GittyHash {
    sha256: [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0,
    ],
};

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct GittyTreeRef {
    pub hash: GittyHash,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct GittyBlobRef {
    pub hash: GittyHash,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct GittyCommitRef {
    pub hash: GittyHash,
}

#[derive(PartialEq, Eq, Hash)]
pub enum GittyObjectRef<'a> {
    Tree(&'a GittyTreeRef),
    Blob(&'a GittyBlobRef),
    Commit(&'a GittyCommitRef),
}

impl<'a> GittyObjectRef<'a> {
    pub fn to_owned(&self) -> OwnedGittyObjectRef {
        OwnedGittyObjectRef::from(self)
    }
}

#[derive(PartialEq, Eq, Hash)]
pub enum OwnedGittyObjectRef {
    Tree(GittyTreeRef),
    Blob(GittyBlobRef),
    Commit(GittyCommitRef),
}
impl<'a> From<&'a GittyObjectRef<'a>> for OwnedGittyObjectRef {
    fn from(a: &GittyObjectRef<'a>) -> OwnedGittyObjectRef {
        // use GittyObjectRef::*;
        match a {
            GittyObjectRef::Tree(t) => OwnedGittyObjectRef::Tree((*t).clone()),
            GittyObjectRef::Blob(t) => OwnedGittyObjectRef::Blob((*t).clone()),
            GittyObjectRef::Commit(t) => OwnedGittyObjectRef::Commit((*t).clone()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GittyTreeEntry {
    #[serde(rename = "tree")]
    Tree(GittyTreeMetadata),
    #[serde(rename = "blob")]
    Blob(GittyBlobMetadata),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GittyTree {
    pub entries: Vec<GittyTreeEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GittyAuthor {
    pub name: String,
    pub email: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]

pub struct GittyCommit {
    // author / committer are unused, added for
    pub author: GittyAuthor,
    pub committer: GittyAuthor,
    pub author_time: DateTime<FixedOffset>,
    pub commit_time: DateTime<FixedOffset>,
    pub message: String,
    pub depth: u64,
    // should be Vec<GittyCommitRef> and root: GittyTreeRef but then serialization looks ugly
    pub parents: Vec<GittyHash>,
    pub root: GittyHash,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GittyTreeMetadata {
    #[serde(with = "serde_compact_osstr")]
    pub name: OsString,
    pub modified: DateTime<Utc>,
    pub permissions: Permissions,
    pub hash: GittyHash,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GittyBlobMetadata {
    #[serde(with = "serde_compact_osstr")]
    pub name: OsString,
    pub modified: DateTime<Utc>,
    pub permissions: Permissions,
    pub size: u64,
    pub is_symlink: bool, // blob contains symlink target as text
    pub hash: GittyHash,
}

// TODO: windows compat
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Permissions {
    pub kind: String,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
}

impl Permissions {
    pub fn new(m: &fs::Metadata) -> Permissions {
        Permissions {
            kind: "unix".to_owned(),
            mode: m.mode(),
            uid: m.uid(),
            gid: m.gid(),
        }
    }
}
pub struct GittyError {
    pub prefix: String,
    pub inner: Box<Display>,
}
impl GittyError {
    pub fn new(prefix: String, inner: Box<Display>) -> GittyError {
        GittyError { prefix, inner }
    }
}
impl Display for GittyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.prefix, self.inner)
    }
}

impl Debug for GittyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.prefix, self.inner)
    }
}
