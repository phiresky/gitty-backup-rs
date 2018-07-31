use chrono;
use chrono::DateTime;
use chrono::FixedOffset;
use chrono::TimeZone;
use database::GittyDatabase;
use fs_walk;
use model::*;
use std::path::Path;
use whoami;

pub fn write_commit(
    db: &mut GittyDatabase,
    root: GittyTreeRef,
    parent_ref: GittyCommitRef,
) -> Result<GittyCommitRef, GittyError> {
    let parent = db.load_commit(&parent_ref)?;
    db.store_commit(create_commit(root, vec![parent_ref.hash], parent.depth + 1))
        .map_err(|e| GittyError::from(e))
}

pub fn create_commit(root: GittyTreeRef, parents: Vec<GittyHash>, depth: u64) -> GittyCommit {
    let author = default_author();
    let commit_time = now();
    GittyCommit {
        committer: author.clone(),
        author,
        parents,
        message: String::from("automatic commit"),
        depth,
        commit_time,
        author_time: commit_time,
        root: root.hash,
    }
}

pub fn commit_current_state_to_head(
    path: &Path,
    db: &mut impl GittyDatabase,
    ignorepath: &Path,
) -> Result<GittyCommitRef, GittyError> {
    let root = fs_walk::recursive_write_tree_to_db(path, db, &ignorepath)
        .map_err(|p| format!("{}", p))
        .unwrap();
    let old_head = db.get_head_commit()?;
    let commit_ref = write_commit(db, root, old_head)?;
    db.update_head_commit(&commit_ref)?;
    Ok(commit_ref)
}

fn now() -> DateTime<FixedOffset> {
    let local = chrono::Local::now();
    local.with_timezone(local.offset())
}

fn default_author() -> GittyAuthor {
    let name = whoami::username();
    GittyAuthor {
        email: format!("{}@{}", name.clone(), whoami::hostname()),
        name,
    }
}

pub struct CommitWalker<'a> {
    db: &'a GittyDatabase,
    current: GittyCommitRef,
}
impl<'a> Iterator for CommitWalker<'a> {
    type Item = Result<(GittyCommitRef, GittyCommit), GittyError>;
    fn next(&mut self) -> Option<Self::Item> {
        match self.db.load_commit(&self.current) {
            Err(e) => Some(Err(GittyError::from(e))),
            Ok(commit) => {
                if commit.depth == 0 {
                    // done, ignore first (empty) commit
                    None
                } else if commit.parents.len() != 1 {
                    Some(Err(GittyError::new(
                        "CommitWalker".to_string(),
                        Box::new("multiple parents not supported"),
                    )))
                } else {
                    let commit_ref = self.current.clone();
                    self.current = GittyCommitRef {
                        hash: commit.parents[0].clone(),
                    };
                    Some(Ok((commit_ref, commit)))
                }
            }
        }
    }
}
pub fn walk_commits<'a>(db: &'a GittyDatabase, start: GittyCommitRef) -> CommitWalker<'a> {
    CommitWalker { db, current: start }
}
