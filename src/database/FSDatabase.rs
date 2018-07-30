use database::*;
use digest::Digest;
use hex;
use model::GittyObjectRef::Blob;
use model::GittyObjectRef::Tree;
use model::*;
use serde_json;
use std::path::Path;
use std::path::PathBuf;
use std::fs;

pub struct FSDatabaseConfig {
    pub root: PathBuf,
    pub object_prefix_length: usize,
    // hasher: GittyHasher
}
pub struct FSDatabase {
    config: FSDatabaseConfig,
}

impl FSDatabase {
    pub fn new(config: FSDatabaseConfig) -> FSDatabase {
        FSDatabase { config }
    }
}

struct SerializeError {
    serde_error: serde_json::Error,
}
impl DBError for SerializeError {
    fn as_up(&self) -> Box<Display> {
        return Box::new(format!("Serde error: {:?}", self.serde_error));
    }
}
impl DBError for std::io::Error {
    fn as_up(&self) -> Box<Display> {
        return Box::new(format!("IO error: {:?}", self));
    }
}

impl std::convert::From<std::io::Error> for Box<dyn DBError> {
    fn from(c: std::io::Error) -> Box<DBError> {
        let b: Box<DBError> = Box::new(c);
        b
    }
}

fn get_output_path(config: &FSDatabaseConfig, object_ref: &GittyObjectRef) -> PathBuf {
    let mut p = config.root.clone();
    let (hash, parent) = match object_ref {
        Blob(b) => (&b.hash, "file"),
        Tree(t) => (&t.hash, "tree"),
    };
    p.push(parent);
    let mut hash_str = hex::encode(hash.sha256);
    let suffix = hash_str.split_off(config.object_prefix_length);
    p.push(hash_str);
    p.push(suffix);
    return p;
}

impl GittyDatabase for FSDatabase {
    fn store_blob(&mut self, path: &Path) -> Result<GittyBlobRef, Box<DBError>> {
        debug!("DB: store blob {} NOOP", path.to_string_lossy());

        //let blob_ref = GittyObjectRef::Tree(GittyBlobRef { hash });

        //let out_path = get_output_path(&self.config, &blob_ref);
        Ok(GittyBlobRef {
            hash: PLACEHOLDER_HASH,
        })
    }
    fn store_tree(&mut self, tree: GittyTree) -> Result<GittyTreeRef, Box<DBError>> {
        let serialized = serde_json::to_string(&tree).map_err(|e| {
            // TODO: why is the extra step necessary
            let b: Box<DBError> = Box::new(SerializeError { serde_error: e });
            return b;
        })?;
        let mut hasher = get_hasher();
        hasher.input(serialized.as_bytes());
        let hash = hasher_output(hasher);
        let tree_ref = GittyObjectRef::Tree(GittyTreeRef { hash });
        let out_path = get_output_path(&self.config, &tree_ref);
        debug!(
            "DB: stored tree {} as {}",
            out_path.to_string_lossy(),
            serde_json::to_string_pretty(&tree).unwrap(),
        );
        fs::create_dir_all(out_path.parent().unwrap())?;
        fs::write(out_path, serialized)?;
        if let GittyObjectRef::Tree(p) = tree_ref {
            Ok(p)
        } else {
            // TODO: ugly
            panic!()
        }
    }
}
