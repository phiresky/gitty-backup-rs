use database::*;
use digest::Digest;
use hex;
use model::GittyObjectRef::Blob;
use model::GittyObjectRef::Tree;
use rand::OsRng;
use rand::Rng;
use serde_json;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

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

    fn store_symlink(&mut self, in_path: &Path) -> Result<GittyBlobRef, Box<DBError>> {
        debug!("DB: store blob {} NOOP", in_path.to_string_lossy());
        Ok(GittyBlobRef {
            hash: PLACEHOLDER_HASH,
        })
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

fn get_temp_path(config: &FSDatabaseConfig) -> PathBuf {
    let mut p = config.root.clone();
    p.push("temp");
    let mut rng = OsRng::new().unwrap();
    let mut arr = [0u8; 32];
    rng.fill(&mut arr[..]);
    p.push(format!("temp-{}", hex::encode(arr)));
    p
}
const COPY_BUF_SIZE: usize = 1024 * 1024;

// https://doc.rust-lang.org/src/std/io/util.rs.html#48-68
pub fn hashing_copy(
    reader: &mut impl Read,
    writer: &mut impl Write,
    hasher: &mut impl Digest,
) -> std::io::Result<u64> {
    let mut buf = Box::new([0u8; COPY_BUF_SIZE]);

    let mut written = 0;
    loop {
        let len = match reader.read(&mut *buf) {
            Ok(0) => return Ok(written),
            Ok(len) => len,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        let buf_part = &buf[..len];
        hasher.input(buf_part);
        writer.write_all(buf_part)?;
        written += len as u64;
    }
}

impl GittyDatabase for FSDatabase {
    fn store_blob(
        &mut self,
        in_path: &Path,
        is_symlink: bool,
    ) -> Result<GittyBlobRef, Box<DBError>> {
        if is_symlink {
            return self.store_symlink(in_path);
        }
        debug!("DB: store blob {} NOOP", in_path.to_string_lossy());

        let tmp_out_path = get_temp_path(&self.config);
        fs::create_dir_all(tmp_out_path.parent().unwrap())?;

        debug!("copying {:?} to {:?} while hashing", in_path, &tmp_out_path);
        let mut reader = File::open(in_path)?;
        let mut writer = File::create(&tmp_out_path)?;
        let mut hasher = get_hasher();
        hashing_copy(&mut reader, &mut writer, &mut hasher)?;

        let hash = hasher_output(hasher);
        let blob_ref = GittyObjectRef::Blob(GittyBlobRef { hash });

        let out_path = get_output_path(&self.config, &blob_ref);
        debug!("moving {:?} to {:?}", tmp_out_path, out_path);
        fs::create_dir_all(out_path.parent().unwrap())?;

        fs::rename(tmp_out_path, out_path)?;
        if let GittyObjectRef::Blob(p) = blob_ref {
            Ok(p)
        } else {
            // TODO: ugly
            panic!()
        }
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
