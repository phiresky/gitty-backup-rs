use model::*;
use std;
use std::fmt::Display;
use std::path::Path;
use std::path::PathBuf;

pub trait GittyDatabase {
    fn get_head_commit(&self) -> Result<GittyCommitRef, DBError>;
    fn update_head_commit(&self, commit_ref: &GittyCommitRef) -> Result<(), DBError>;
    fn load_blob(&self, blob_ref: &GittyBlobRef) -> Result<PathBuf, DBError>;
    fn load_tree(&self, tree_ref: &GittyTreeRef) -> Result<GittyTree, DBError>;
    fn load_commit(&self, commit_ref: &GittyCommitRef) -> Result<GittyCommit, DBError>;

    fn store_blob(&mut self, path: &Path, is_symlink: bool) -> Result<GittyBlobRef, DBError>;
    fn store_tree(&mut self, tree: GittyTree) -> Result<GittyTreeRef, DBError>;
    fn store_commit(&mut self, commit: GittyCommit) -> Result<GittyCommitRef, DBError>;
}

pub type DBError = Box<dyn _DBError>;

pub trait _DBError {
    // TODO: why is this needed? https://stackoverflow.com/questions/28632968/why-doesnt-rust-support-trait-object-upcasting
    fn as_up(&self) -> Box<Display>;
}

impl std::convert::From<DBError> for GittyError {
    fn from(i: DBError) -> GittyError {
        // let j = i as Box<Display>;
        GittyError::new("DB error".to_owned(), i.as_up())
    }
}
//impl<T: DBError> std::fmt::Debug for T {}

pub mod fs_database;
