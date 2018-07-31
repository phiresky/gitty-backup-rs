extern crate bk_tree;
extern crate chrono;
extern crate env_logger;
extern crate ignore;
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
extern crate whoami;
use gitty_backup_rs::model::GittyError;
use std::path::Path;
extern crate gitty_backup_rs;
use gitty_backup_rs::commits;
use gitty_backup_rs::database;
use gitty_backup_rs::database::GittyDatabase;

fn main() -> Result<(), GittyError> {
    env_logger::init_from_env(
        env_logger::Env::default().filter_or("RUST_LOG", "gitty_backup_rs=info"),
    );
    let args: Vec<_> = std::env::args().collect();
    let path = Path::new(&args[1]);
    let dbpath = Path::new(&args[2]);
    let ignorepath = path.join(".gittyignore");

    let mut db = database::fs_database::FSDatabase::create_or_open(dbpath).unwrap_or_else(|m| {
        panic!("{}", m);
    });
    /*{
        let head = db.get_head_commit()?;
        for commit in commits::walk_commits(&mut db, head) {
            println!("{:?}", commit);
        }
    }*/
    commits::commit_current_state_to_head(path, &mut db, &ignorepath)
        .unwrap_or_else(|m| panic!("{}", m));
    Ok(())
}
