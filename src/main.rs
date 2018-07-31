use std::fmt::Display;
use std::path::Path;

extern crate bk_tree;
extern crate chrono;
extern crate ignore;
extern crate pretty_env_logger;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate walkdir;
#[macro_use]
extern crate log;
extern crate digest;
extern crate hex;
extern crate rand;
extern crate sha2;
use chrono::prelude::*;
use std::cmp::Ordering;

use walkdir::DirEntry;

mod database;
mod model;
mod util;
use model::*;

use database as db;

// use bk_tree::{BKTree, metrics};

fn main() {
    pretty_env_logger::init();
    let args: Vec<_> = std::env::args().collect();
    let path = Path::new(&args[1]);
    let dbpath = Path::new(&args[2]);
    let ignorepath = path.join(".gittyignore");
    find_changes(path, dbpath, &ignorepath);
}

impl std::convert::From<walkdir::Error> for GittyError {
    fn from(i: walkdir::Error) -> GittyError {
        GittyError::new("error while walking".to_owned(), Box::new(i))
    }
}
impl std::convert::From<std::io::Error> for GittyError {
    fn from(i: std::io::Error) -> GittyError {
        GittyError::new("error while walking".to_owned(), Box::new(i))
    }
}

fn dirent_to_gitty_tree_entry(
    database: &mut impl db::GittyDatabase,
    mut entries: Vec<GittyTreeEntry>,
    dirent: DirEntry,
    metadata: std::fs::Metadata,
) -> Result<Vec<GittyTreeEntry>, GittyError> {
    let is_symlink = metadata.file_type().is_symlink();
    if metadata.is_file() || is_symlink {
        let new_entry = GittyTreeEntry::Blob(GittyBlobMetadata {
            name: dirent.file_name().to_os_string(),
            modified: DateTime::from(metadata.modified()?),
            permissions: Permissions::new(&metadata),
            size: metadata.len(),
            is_symlink,
            hash: database.store_blob(dirent.path(), is_symlink)?.hash,
        });
        entries.push(new_entry);
        return Ok(entries);
    }

    if metadata.is_dir() {
        let new_entry = GittyTreeEntry::Tree(GittyTreeMetadata {
            name: dirent.file_name().to_os_string(),
            modified: DateTime::from(metadata.modified()?),
            permissions: Permissions::new(&metadata),
            hash: database.store_tree(GittyTree { entries })?.hash,
        });
        let entries = vec![new_entry];
        return Ok(entries);
    }
    panic!("Unknown file type: {:?}", metadata.file_type());
}

fn find_changes(dir: &Path, dbdir: &Path, ignorefile: &Path) {
    let mut ignore = ignore::gitignore::GitignoreBuilder::new(dir);
    ignore.add(ignorefile);
    let ignorer = ignore.build().unwrap();

    let walker = walkdir::WalkDir::new(dir)
        .follow_links(false)
        .contents_first(true)
        .sort_by(|a, b| {
            // TODO: prevent calling stat mutliple times (prob important for performance!)

            // 1. directories first
            // 2. sort files by name (TODO: OSStr sort consistency?)
            let mut ord = Ordering::Equal;
            if let (Ok(ma), Ok(mb)) = (a.metadata(), b.metadata()) {
                ord = ord.then(ma.is_dir().cmp(&mb.is_dir()).reverse());
            }
            ord.then_with(|| a.file_name().cmp(b.file_name()))
        });
    let mut db = db::FSDatabase::FSDatabase::new(db::FSDatabase::FSDatabaseConfig {
        root: dbdir.to_path_buf(),
        object_prefix_length: 3,
    });
    let mut v: Vec<GittyTreeEntry> = Vec::new();
    let entries = walker
        .into_iter()
        .filter_entry(|e: &DirEntry| {
            let i = ignorer.matched(e.path(), e.file_type().is_dir());
            match i {
                ignore::Match::None => true,
                ignore::Match::Whitelist(_) => true,
                _ => false,
            }
        })
        .filter_map(|entry| match entry {
            Err(p) => {
                warn!("error accessing: {:?}, ignoring", p);
                return None;
            }
            Ok(dirent) => {
                println!("{}", dirent.path().to_string_lossy());
                match dirent.metadata() {
                    Ok(m) => Some((dirent, m)),
                    Err(e) => {
                        warn!("error accessing: {:?}, ignoring ({:?})", dirent, e);
                        None
                    }
                }
            }
        })
        .try_fold(v, |entries, (entry, metadata)| {
            dirent_to_gitty_tree_entry(&mut db, entries, entry, metadata)
        });

    match entries {
        Ok(entries) => {
            if entries.len() > 1 {
                eprintln!("more than one root? {:#?}", entries);
                panic!();
            }
            let root = &entries[0];
            if let GittyTreeEntry::Tree(t) = root {
                println!("root: {:?}", t.hash);
            } else {
                panic!("root is blob?");
            }
        }
        Err(e) => println!("{}", e),
    }
}
