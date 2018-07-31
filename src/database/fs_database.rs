use commits::create_commit;
use database::*;
use digest::Digest;
use hex;
use model::GittyObjectRef::*;
use rand::OsRng;
use rand::Rng;
use serde_json;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

#[derive(Clone)]
pub struct FSDatabaseConfig {
    pub root: PathBuf,
    pub object_prefix_length: usize,
    // hasher: GittyHasher
}
pub struct FSDatabase {
    config: FSDatabaseConfig,
}

impl FSDatabase {
    pub fn open(config: FSDatabaseConfig) -> Option<FSDatabase> {
        let db = FSDatabase { config };
        if db.head_path().exists() {
            Some(db)
        } else {
            None
        }
    }
    pub fn create(config: FSDatabaseConfig) -> Result<FSDatabase, GittyError> {
        if config.root.exists() {
            Err(GittyError::new(
                String::from("Creation"),
                Box::new(format!("{} already exists", config.root.display())),
            ))
        } else {
            let mut db = FSDatabase { config };
            let empty_tree = db.store_tree(GittyTree { entries: vec![] })?;
            let first_commit = create_commit(empty_tree, vec![], 0);
            let commit_ref = db.store_commit(first_commit)?;
            db.update_head_commit(&commit_ref)?;
            Ok(db)
        }
    }

    pub fn create_or_open(dbdir: &Path) -> Result<impl GittyDatabase, impl Display> {
        let config = FSDatabaseConfig {
            root: dbdir.to_path_buf(),
            object_prefix_length: 3,
        };
        FSDatabase::open(config.clone()).ok_or("no").or_else(|_| {
            info!("Creating new database in {}", dbdir.to_path_buf().display());
            FSDatabase::create(config)
        })
    }

    fn store_symlink(&mut self, in_path: &Path) -> Result<GittyBlobRef, DBError> {
        debug!("DB: store blob {} NOOP", in_path.to_string_lossy());
        Ok(GittyBlobRef {
            hash: PLACEHOLDER_HASH,
        })
    }

    fn head_path(&self) -> PathBuf {
        self.config.root.join("HEAD")
    }
}

struct SerializeError {
    serde_error: serde_json::Error,
}
impl _DBError for SerializeError {
    fn as_up(&self) -> Box<Display> {
        return Box::new(format!("Serde error: {:?}", self.serde_error));
    }
}
impl _DBError for std::io::Error {
    fn as_up(&self) -> Box<Display> {
        return Box::new(format!("IO error: {:?}", self));
    }
}

impl std::convert::From<std::io::Error> for DBError {
    fn from(c: std::io::Error) -> DBError {
        let b: DBError = Box::new(c);
        b
    }
}

fn get_object_path(config: &FSDatabaseConfig, object_ref: &GittyObjectRef) -> PathBuf {
    let mut p = config.root.clone();
    let (hash, parent) = match object_ref {
        Blob(b) => (&b.hash, "file"),
        Tree(t) => (&t.hash, "tree"),
        Commit(c) => (&c.hash, "commit"),
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

fn wrap_serde_err(e: serde_json::Error) -> DBError {
    // TODO: why is this extra step necessary
    let b: DBError = Box::new(SerializeError { serde_error: e });
    return b;
}
impl GittyDatabase for FSDatabase {
    fn store_blob(&mut self, in_path: &Path, is_symlink: bool) -> Result<GittyBlobRef, DBError> {
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
        let blob_ref = GittyBlobRef { hash };

        let out_path = get_object_path(&self.config, &GittyObjectRef::Blob(&blob_ref));
        debug!("moving {:?} to {:?}", tmp_out_path, out_path);
        fs::create_dir_all(out_path.parent().unwrap())?;

        fs::rename(tmp_out_path, out_path)?;
        Ok(blob_ref)
    }

    fn store_tree(&mut self, tree: GittyTree) -> Result<GittyTreeRef, DBError> {
        let serialized = serde_json::to_string(&tree).map_err(wrap_serde_err)?;
        let mut hasher = get_hasher();
        hasher.input(serialized.as_bytes());
        let hash = hasher_output(hasher);
        let tree_ref = GittyTreeRef { hash };
        let out_path = get_object_path(&self.config, &GittyObjectRef::Tree(&tree_ref));
        debug!(
            "DB: stored tree {} as {}",
            out_path.to_string_lossy(),
            serde_json::to_string_pretty(&tree).unwrap(),
        );
        fs::create_dir_all(out_path.parent().unwrap())?;
        fs::write(out_path, serialized)?;
        Ok(tree_ref)
    }

    // TODO: code duplication with store_tree
    fn store_commit(&mut self, commit: GittyCommit) -> Result<GittyCommitRef, DBError> {
        let serialized = serde_json::to_string(&commit).map_err(wrap_serde_err)?;
        let mut hasher = get_hasher();
        hasher.input(serialized.as_bytes());
        let hash = hasher_output(hasher);
        let commit_ref = GittyCommitRef { hash };
        let out_path = get_object_path(&self.config, &GittyObjectRef::Commit(&commit_ref));
        debug!(
            "DB: stored commit {} as {}",
            out_path.to_string_lossy(),
            serde_json::to_string_pretty(&commit).unwrap(),
        );
        fs::create_dir_all(out_path.parent().unwrap())?;
        fs::write(out_path, serialized)?;
        Ok(commit_ref)
    }

    fn load_blob(&self, blob_ref: &GittyBlobRef) -> Result<PathBuf, DBError> {
        let path = get_object_path(&self.config, &GittyObjectRef::Blob(blob_ref));
        Ok(path)
    }
    fn load_tree(&self, tree_ref: &GittyTreeRef) -> Result<GittyTree, DBError> {
        let path = get_object_path(&self.config, &GittyObjectRef::Tree(tree_ref));
        let reader = File::open(path)?;
        let res = serde_json::from_reader(reader).map_err(wrap_serde_err)?;
        Ok(res)
    }
    fn load_commit(&self, commit_ref: &GittyCommitRef) -> Result<GittyCommit, DBError> {
        let path = get_object_path(&self.config, &GittyObjectRef::Commit(commit_ref));
        let reader = File::open(path)?;
        let res = serde_json::from_reader(reader).map_err(wrap_serde_err)?;
        Ok(res)
    }

    fn get_head_commit(&self) -> Result<GittyCommitRef, DBError> {
        let head_path = self.head_path();
        Ok(serde_json::from_reader(File::open(head_path)?).map_err(wrap_serde_err)?)
    }
    fn update_head_commit(&self, commit_ref: &GittyCommitRef) -> Result<(), DBError> {
        let head_path = self.head_path();
        serde_json::to_writer(File::create(head_path)?, commit_ref).map_err(wrap_serde_err)?;
        Ok(())
    }
}
