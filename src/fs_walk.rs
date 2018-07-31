use chrono::prelude::*;
use database as db;
use ignore;
use model::*;
use std;
use std::cmp::Ordering;
use std::ffi::OsString;
use std::path::Component as PathComponent;
use std::path::Path;
use walkdir;
use walkdir::DirEntry;

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

struct StackPart {
    name: OsString,
    metadata: std::fs::Metadata,
    entries: Vec<GittyTreeEntry>,
}

fn create_tree_entry(
    database: &mut impl db::GittyDatabase,
    StackPart {
        name,
        metadata,
        entries,
    }: StackPart,
) -> Result<GittyTreeEntry, GittyError> {
    Ok(GittyTreeEntry::Tree(GittyTreeMetadata {
        name,
        modified: DateTime::from(metadata.modified()?),
        permissions: Permissions::new(&metadata),
        hash: database.store_tree(GittyTree { entries })?.hash,
    }))
}
fn ascend_path_stack(
    database: &mut impl db::GittyDatabase,
    path_stack: &mut Vec<StackPart>,
    i: usize,
) -> Result<(), GittyError> {
    while path_stack.len() > i {
        println!(
            "ascending push {}",
            path_stack.last().unwrap().name.to_string_lossy()
        );
        let new_entry = create_tree_entry(database, path_stack.pop().unwrap())?;
        path_stack.last_mut().unwrap().entries.push(new_entry);
    }
    Ok(())
}
fn dirent_to_gitty_tree_entry(
    database: &mut impl db::GittyDatabase,
    path_stack: &mut Vec<StackPart>,
    dirent: DirEntry,
    metadata: std::fs::Metadata,
) -> Result<(), GittyError> {
    // TODO: this is kinda ugly, use zip / functional stuff?
    let current_path: Vec<PathComponent> = dirent.path().components().collect();
    let first_diff = path_stack
        .into_iter()
        .enumerate()
        .find(|(i, StackPart { name: seg_a, .. })| {
            let seg_b = current_path.get(*i);
            Some(&PathComponent::Normal(seg_a)) != seg_b
        })
        .map(|(i, _)| i);
    if let Some(i) = first_diff {
        ascend_path_stack(database, path_stack, i)?;
    }

    if current_path.len() != path_stack.len() + 1 {
        panic!(
            "cannot descend multiple {:?} -> {:?}",
            "?", //path_stack.iter().map(|e| e.name),
            current_path
        );
    }
    let is_symlink = metadata.file_type().is_symlink();
    if metadata.is_dir() {
        let name: OsString = dirent.file_name().to_os_string();
        assert!(current_path.last().unwrap().clone() == PathComponent::Normal(&name));
        println!("descending into {}", name.to_string_lossy());
        let entries: Vec<GittyTreeEntry> = Vec::new();
        path_stack.push(StackPart {
            name,
            metadata,
            entries,
        });
    } else if metadata.is_file() || is_symlink {
        let new_entry = GittyTreeEntry::Blob(GittyBlobMetadata {
            name: dirent.file_name().to_os_string(),
            modified: DateTime::from(metadata.modified()?),
            permissions: Permissions::new(&metadata),
            size: metadata.len(),
            is_symlink,
            hash: database.store_blob(dirent.path(), is_symlink)?.hash,
        });
        path_stack.last_mut().unwrap().entries.push(new_entry);
    } else {
        panic!("Unknown file type: {:?}", metadata.file_type());
    }
    Ok(())
}

pub fn recursive_write_tree_to_db(
    dir: &Path,
    db: &mut impl db::GittyDatabase,
    ignorefile: &Path,
) -> Result<GittyTreeRef, GittyError> {
    let mut ignore = ignore::gitignore::GitignoreBuilder::new(dir);
    ignore.add(ignorefile);
    let ignorer = ignore.build().unwrap();

    let walker = walkdir::WalkDir::new(dir)
        .follow_links(false)
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
    let mut path_stack: Vec<StackPart> = Vec::new();
    for (entry, metadata) in walker
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
        }) {
        dirent_to_gitty_tree_entry(db, &mut path_stack, entry, metadata)?;
    }
    ascend_path_stack(db, &mut path_stack, 1)?;
    let root_entry = path_stack.pop().unwrap();
    if path_stack.len() != 0 {
        panic!("root invalid");
    }
    let root = create_tree_entry(db, root_entry)?;
    if let GittyTreeEntry::Tree(t) = root {
        return Ok(GittyTreeRef { hash: t.hash });
    } else {
        panic!("root is blob?");
    }
}
