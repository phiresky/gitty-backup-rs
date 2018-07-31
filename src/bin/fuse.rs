extern crate bidir_map;
extern crate bimap;
extern crate chrono;
extern crate env_logger;
extern crate fuse;
extern crate gitty_backup_rs;
extern crate libc;
extern crate lru_time_cache;
extern crate time;

use bimap::BiMap;
use chrono::Utc;
use fuse::FileAttr;
use fuse::FileType;
use fuse::Filesystem;
use fuse::ReplyAttr;
use fuse::ReplyData;
use fuse::ReplyDirectory;
use fuse::ReplyEntry;
use fuse::Request;
use fuse::FUSE_ROOT_ID;
use gitty_backup_rs::commits::walk_commits;
use gitty_backup_rs::database::fs_database::FSDatabase;
use gitty_backup_rs::database::GittyDatabase;
use gitty_backup_rs::model::*;
use libc::EINVAL;
use libc::EIO;
use libc::EISDIR;
use libc::ENOENT;
use lru_time_cache::LruCache;
use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::env;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fs::File;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::path::Path;
use std::time::Duration;
use std::time::SystemTime;
use time::Timespec;
type Inode = u64;
const TTL: Timespec = Timespec {
    sec: 2 << 62,
    nsec: 0,
}; // immutable, cache forever

const STD_ATTR: FileAttr = FileAttr {
    ino: 0,
    atime: Timespec { sec: 0, nsec: 0 },
    mtime: Timespec { sec: 0, nsec: 0 },
    ctime: Timespec { sec: 0, nsec: 0 },
    crtime: Timespec { sec: 0, nsec: 0 },
    blocks: 0,
    size: 0,
    flags: 0,
    gid: 0,
    uid: 0,
    perm: 0o755,
    nlink: 1,
    kind: FileType::Directory,
    rdev: 0,
};
struct GittyViewer<'a> {
    db: &'a GittyDatabase,
    // TODO: BidirMap is really slow
    inode_commits: BiMap<Inode, GittyCommitRef>,
    commits: HashMap<GittyCommitRef, GittyCommit>,
    root_map: HashMap<OsString, GittyCommitRef>,
    // map inode <-> parent tree, name, content hash
    inode_trees_blobs: BiMap<Inode, (GittyTreeRef, OsString, OwnedGittyObjectRef)>,
    trees: HashMap<GittyTreeRef, GittyTree>,
    inode_max: Inode,
    root_mtime: Duration,
    blob_read_cache: LruCache<Inode, File>,
}

fn find_tree_entry<'a>(tree: &'a GittyTree, name: &'a OsStr) -> Option<&'a GittyTreeEntry> {
    tree.entries.iter().find(|e| {
        name == match e {
            GittyTreeEntry::Tree(p) => &p.name,
            GittyTreeEntry::Blob(p) => &p.name,
        }
    })
}

fn commit_fname(commit: &GittyCommit) -> String {
    (commit.commit_time
        - chrono::Duration::nanoseconds(commit.commit_time.timestamp_subsec_nanos() as i64))
        .to_string()
}
impl<'a> GittyViewer<'a> {
    fn new(db: &'a mut GittyDatabase) -> GittyViewer<'a> {
        GittyViewer {
            db,
            inode_trees_blobs: BiMap::new(),
            inode_commits: BiMap::new(),
            inode_max: 1,
            root_mtime: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap(),
            trees: HashMap::new(),
            commits: HashMap::new(),
            root_map: HashMap::new(),
            blob_read_cache: LruCache::with_expiry_duration_and_capacity(
                Duration::from_secs(60),
                500,
            ),
        }
    }
    fn get_tree(&mut self, r: &GittyTreeRef) -> Option<&GittyTree> {
        match self.trees.entry(r.clone()) {
            Entry::Occupied(o) => Some(o.into_mut()),
            Entry::Vacant(v) => match self.db.load_tree(r) {
                Ok(e) => Some(v.insert(e)),
                Err(_) => None,
            },
        }
    }
    fn tree_ele_to_inode(
        &mut self,
        tree: &GittyTree,
        tree_ref: &GittyTreeRef,
        name: &OsStr,
    ) -> Inode {
        let entry = find_tree_entry(tree, name).unwrap();
        self.tree_entry_to_inode(tree_ref, entry)
    }
    fn tree_entry_to_inode(&mut self, tree_ref: &GittyTreeRef, entry: &GittyTreeEntry) -> Inode {
        let (name, entry_wrap) = match entry {
            GittyTreeEntry::Tree(t) => (
                t.name.clone(),
                OwnedGittyObjectRef::Tree(GittyTreeRef {
                    hash: t.hash.clone(),
                }),
            ),
            GittyTreeEntry::Blob(t) => (
                t.name.clone(),
                OwnedGittyObjectRef::Blob(GittyBlobRef {
                    hash: t.hash.clone(),
                }),
            ),
        };

        let tp = (tree_ref.clone(), name.to_owned(), entry_wrap);
        if let Some(inode) = self.inode_trees_blobs.get_by_right(&tp) {
            return *inode;
        }

        self.inode_max += 1;
        let inode = self.inode_max;
        self.inode_trees_blobs.insert(inode, tp);
        inode
    }
    fn commit_to_inode(
        &mut self,
        commit_ref: Cow<GittyCommitRef>,
        commit: Cow<GittyCommit>,
    ) -> Inode {
        if let Some(inode) = self.inode_commits.get_by_right(&commit_ref) {
            return *inode;
        }

        self.inode_max += 1;
        let inode = self.inode_max;
        self.inode_commits
            .insert(inode, commit_ref.as_ref().clone());
        self.commits
            .insert(commit_ref.into_owned(), commit.into_owned());
        inode
    }
    fn inode_to_tree(&self, inode: Inode) -> Option<Cow<GittyTreeEntry>> {
        self.inode_trees_blobs
            .get_by_left(&inode)
            .and_then(|(r, n, _)| self.trees.get(r).map(|p| (p, n.as_ref())))
            .and_then(|(a, b)| find_tree_entry(a, b))
            .map(|p| Cow::Borrowed(p))
            .or_else(|| {
                self.inode_commits
                    .get_by_left(&inode)
                    .and_then(|r| self.commits.get(r))
                    .and_then(|c| {
                        Some(Cow::Owned(GittyTreeEntry::Tree(GittyTreeMetadata {
                            hash: c.root.clone(),
                            name: OsStr::new(&commit_fname(&c)).to_owned(),
                            modified: c.commit_time.with_timezone(&Utc),
                            permissions: Permissions {
                                kind: "unix".to_owned(),
                                mode: 0o755,
                                uid: 0,
                                gid: 0,
                            },
                        })))
                    })
            })
    }

    fn entry_to_attr(entry: &GittyTreeEntry, ino: Inode) -> FileAttr {
        match entry {
            GittyTreeEntry::Tree(t) => {
                let time = Timespec {
                    sec: t.modified.timestamp(),
                    nsec: t.modified.timestamp_subsec_nanos() as i32,
                };
                FileAttr {
                    ino,
                    atime: time,
                    mtime: time,
                    ctime: time,
                    crtime: time,
                    blocks: 0,
                    size: 0,
                    flags: 0,
                    gid: t.permissions.gid,
                    uid: t.permissions.uid,
                    perm: t.permissions.mode as u16,
                    nlink: 1,
                    kind: FileType::Directory,
                    rdev: 0,
                }
            }
            GittyTreeEntry::Blob(t) => {
                let time = Timespec {
                    sec: t.modified.timestamp(),
                    nsec: t.modified.timestamp_subsec_nanos() as i32,
                };
                FileAttr {
                    ino,
                    atime: time,
                    mtime: time,
                    ctime: time,
                    crtime: time,
                    size: t.size,
                    gid: t.permissions.gid,
                    uid: t.permissions.uid,
                    perm: t.permissions.mode as u16,
                    nlink: 1,
                    kind: if t.is_symlink {
                        FileType::Symlink
                    } else {
                        FileType::RegularFile
                    },
                    rdev: 0,
                    ..STD_ATTR
                }
            }
        }
    }
}

const GENERATION: u64 = 0;
impl<'a> Filesystem for GittyViewer<'a> {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let tree_ref = {
            if parent == FUSE_ROOT_ID {
                if let Some(commit_ref) = self.root_map.get(name).map(|p| p.clone()) {
                    let c = self.commits.get(&commit_ref).unwrap().clone();
                    let ino = self.commit_to_inode(Cow::Owned(commit_ref), Cow::Owned(c));
                    if let Some(entry) = self.inode_to_tree(ino) {
                        let attr = GittyViewer::entry_to_attr(&entry, ino);
                        reply.entry(&TTL, &attr, GENERATION);
                        return;
                    }
                    reply.error(ENOENT);
                    return;
                //GittyTreeRef { self.commits.get(commit_ref).unwrap().root }
                } else {
                    reply.error(ENOENT);
                    return;
                }
            } else if let Some((_, _, hash)) = self.inode_trees_blobs.get_by_left(&parent) {
                if let OwnedGittyObjectRef::Tree(t) = hash {
                    t.clone()
                } else {
                    panic!("not a dir");
                }
            } else if let Some(x) = self.inode_commits.get_by_left(&parent) {
                let hash = self.commits.get(x).unwrap().root.clone();
                GittyTreeRef { hash }
            } else {
                reply.error(ENOENT);
                return;
            }
        };
        let (tree, entry) = {
            if let Some(tree) = self.get_tree(&tree_ref) {
                if let Some(entry) = find_tree_entry(&tree, name) {
                    (tree.clone(), entry.clone())
                } else {
                    reply.error(ENOENT);
                    return;
                }
            } else {
                reply.error(ENOENT);
                return;
            }
        };
        let ino = self.tree_ele_to_inode(&tree, &tree_ref, name);
        let attr = GittyViewer::entry_to_attr(&entry, ino);
        reply.entry(&TTL, &attr, GENERATION);
        return;
    }

    fn getattr(&mut self, req: &Request, ino: u64, reply: ReplyAttr) {
        if ino == FUSE_ROOT_ID {
            let time = Timespec {
                sec: self.root_mtime.as_secs() as i64,
                nsec: self.root_mtime.subsec_nanos() as i32,
            };

            reply.attr(
                &TTL,
                &FileAttr {
                    ino: FUSE_ROOT_ID,
                    atime: time,
                    mtime: time,
                    ctime: time,
                    crtime: time,
                    gid: req.gid(),
                    uid: req.uid(),
                    perm: 0o755,
                    kind: FileType::Directory,
                    ..STD_ATTR
                },
            );
            return;
        }
        if let Some(entry) = self.inode_to_tree(ino) {
            let attr = GittyViewer::entry_to_attr(&entry, ino);
            reply.attr(&TTL, &attr);
            return;
        }
        reply.error(ENOENT);
    }

    // TODO: implement proper state based i/o
    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        reply: ReplyData,
    ) {
        use lru_time_cache::Entry;

        let mut f = match self.blob_read_cache.entry(ino) {
            Entry::Occupied(o) => o.into_mut(),
            Entry::Vacant(v) => {
                if let Some((_, _, hash)) = self.inode_trees_blobs.get_by_left(&ino) {
                    match hash {
                        OwnedGittyObjectRef::Tree(t) => {
                            reply.error(EISDIR);
                            return;
                        }
                        OwnedGittyObjectRef::Commit(t) => {
                            reply.error(EINVAL);
                            return;
                        }
                        OwnedGittyObjectRef::Blob(blob_ref) => {
                            if let Ok(path) = self.db.load_blob(blob_ref) {
                                v.insert(File::open(path).unwrap())
                            } else {
                                reply.error(EIO);
                                return;
                            }
                        }
                    }
                } else {
                    reply.error(ENOENT);
                    return;
                }
            }
        };
        f.seek(SeekFrom::Start(offset as u64)).unwrap();
        let mut buf = vec![0; size as usize];
        match f.read(&mut buf[..]) {
            Ok(size) => {
                reply.data(&buf[0..size]);
                return;
            }
            Err(e) => {
                eprintln!("read: {:?}", e);
                reply.error(EIO);
                return;
            }
        }
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        if ino == FUSE_ROOT_ID {
            match self.db.get_head_commit() {
                Ok(head) => {
                    for (i, commit) in walk_commits(self.db, head)
                        .enumerate()
                        .skip(offset as usize)
                    {
                        match commit {
                            Ok((commit_ref, commit)) => {
                                //let r = GittyTreeRef { hash: commit.root };
                                //let root = GittyObjectRef::Tree(&r);
                                let fname = commit_fname(&commit);
                                self.root_map
                                    .insert(OsStr::new(&fname).to_owned(), commit_ref.clone());
                                let inode = self
                                    .commit_to_inode(Cow::Owned(commit_ref), Cow::Owned(commit));
                                let full =
                                    reply.add(inode, (i + 1) as i64, FileType::Directory, fname);
                                if full {
                                    reply.ok();
                                    return;
                                }
                            }
                            Err(e) => {
                                eprintln!("walk_commits: {:?}", e);
                                reply.error(EIO);
                                return;
                            }
                        };
                    }
                    reply.ok();
                }
                Err(e) => {
                    eprintln!("get_head: {:?}", GittyError::from(e));
                    reply.error(EIO);
                }
            }
        } else {
            let tree_ref = {
                if let Some((_, _, tree_hash)) = self.inode_trees_blobs.get_by_left(&ino) {
                    match tree_hash {
                        OwnedGittyObjectRef::Tree(t) => t.clone(),
                        _ => {
                            reply.error(EINVAL);
                            return;
                        }
                    }
                } else if let Some(commit_ref) = self.inode_commits.get_by_left(&ino) {
                    GittyTreeRef {
                        hash: self.commits.get(commit_ref).unwrap().root.clone(),
                    }
                } else {
                    reply.error(ENOENT);
                    return;
                }
            };
            let tree = (*self.get_tree(&tree_ref).unwrap()).clone();
            for (i, entry) in tree.entries.iter().enumerate().skip(offset as usize) {
                let ino = self.tree_entry_to_inode(&tree_ref, entry);
                let (name, kind) = match entry {
                    GittyTreeEntry::Tree(t) => (t.name.clone(), FileType::Directory),
                    GittyTreeEntry::Blob(t) => (
                        t.name.clone(),
                        if t.is_symlink {
                            FileType::Symlink
                        } else {
                            FileType::RegularFile
                        },
                    ),
                };
                let full = reply.add(ino, (i + 1) as i64, kind, name);
                if full {
                    reply.ok();
                    return;
                }
            }
            reply.ok();
        }
    }
}

fn main() {
    env_logger::init();
    let dbdir = env::args().nth(1).unwrap();
    let mountpoint = env::args().nth(2).unwrap();
    let mut options: Vec<String> = ["-o", "ro", "-o", "auto_unmount", "-o"]
        .into_iter()
        .map(|p| String::from(*p))
        .collect();

    let mut db = FSDatabase::create_or_open(Path::new(&dbdir)).unwrap_or_else(|m| {
        panic!("{}", m);
    });
    options.push(format!("fsname=gitty:{}", dbdir));
    let options = options.iter().map(|o| o.as_ref()).collect::<Vec<&OsStr>>();
    fuse::mount(GittyViewer::new(&mut db), &mountpoint, &options).unwrap();
}
