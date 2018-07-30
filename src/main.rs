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
extern crate hex;
extern crate digest;
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
    find_changes(path, dbpath);
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
) -> Result<Vec<GittyTreeEntry>, GittyError> {
    let metadata = dirent.metadata()?;
    if metadata.is_file() || metadata.file_type().is_symlink() {
        let new_entry = GittyTreeEntry::Blob(GittyBlobMetadata {
            name: dirent.file_name().to_os_string(),
            modified: DateTime::from(metadata.modified()?),
            permissions: Permissions::from(metadata.permissions()),
            size: metadata.len(),
            is_symlink: metadata.file_type().is_symlink(),
            hash: database.store_blob(dirent.path())?.hash,
        });
        entries.push(new_entry);
        return Ok(entries);
    }

    if metadata.is_dir() {
        let new_entry = GittyTreeEntry::Tree(GittyTreeMetadata {
            name: dirent.file_name().to_os_string(),
            modified: DateTime::from(metadata.modified()?),
            permissions: Permissions::from(metadata.permissions()),
            hash: database.store_tree(GittyTree { entries })?.hash,
        });
        let entries = vec![new_entry];
        return Ok(entries);
    }
    panic!("Unknown file type: {:?}", metadata.file_type());
}

fn find_changes(dir: &Path, dbdir: &Path) {
    //let mut walker = ignore::WalkBuilder::new(dir);
    //walker.standard_filters(false);
    /*walker.sort_by_file_name(|a, b| {
        println!("cmp {:?} {:?}", a, b);
        a.cmp(b)
    });
    if let Some(f) = ignores_file {
        let e = walker.add_ignore(f);
        eprintln!("added ignore {:?}", f);
        if let Some(e) = e {
            println!("{:?}", e);
            panic!();
        }
    }*/
    let walker = walkdir::WalkDir::new(dir)
        .follow_links(false)
        .contents_first(true)
        .sort_by(|a, b| {
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
        object_prefix_length: 3
    });
    let mut v: Vec<GittyTreeEntry> = Vec::new();
    let entries = walker
        .into_iter()
        .try_fold(v, |entries, entry| match entry {
            Err(p) => {
                eprintln!("error accessing: {:?}, ignoring", p);
                return Ok(entries);
            }
            Ok(dirent) => {
                return dirent_to_gitty_tree_entry(&mut db, entries, dirent);
            }
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
        },
        Err(e) => println!("{}", e),
    }
}
