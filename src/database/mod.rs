use model::*;
use std;
use std::fmt::Debug;
use std::fmt::Display;
use std::path::Path;

pub trait GittyDatabase {
    fn store_blob(&mut self, path: &Path, is_symlink: bool) -> Result<GittyBlobRef, Box<DBError>>;
    fn store_tree(&mut self, tree: GittyTree) -> Result<GittyTreeRef, Box<DBError>>;
}

pub trait DBError {
    // TODO: why is this needed? https://stackoverflow.com/questions/28632968/why-doesnt-rust-support-trait-object-upcasting
    fn as_up(&self) -> Box<Display>;
}

impl std::convert::From<Box<DBError>> for GittyError {
    fn from(i: Box<DBError>) -> GittyError {
        // let j = i as Box<Display>;
        GittyError::new("DB error".to_owned(), i.as_up())
    }
}
//impl<T: DBError> std::fmt::Debug for T {}

pub mod FSDatabase;
