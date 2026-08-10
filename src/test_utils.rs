use git2::{Commit, Repository};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

pub struct TestRepo {
    pub path: PathBuf,
    pub repo: Repository,
}

impl TestRepo {
    pub fn new() -> Self {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "git_hierarchy_test_{}_{}",
            std::process::id(),
            id
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();

        let repo = Repository::init(&path).unwrap();
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();

        TestRepo { path, repo }
    }

    pub fn create_commit<'repo>(
        &'repo self,
        message: &str,
        parents: &[&Commit<'_>],
    ) -> Commit<'repo> {
        create_commit(&self.repo, message, parents)
    }

    pub fn create_initial_commit<'repo>(&'repo self) -> Commit<'repo> {
        let commit = self.create_commit("initial commit", &[]);
        let _ = self.repo.branch("main", &commit, false);
        commit
    }
}

impl Default for TestRepo {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn create_commit<'repo>(
    repo: &'repo Repository,
    message: &str,
    parents: &[&Commit<'_>],
) -> Commit<'repo> {
    let sig = repo.signature().unwrap();
    let mut index = repo.index().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();

    let oid = repo
        .commit(None, &sig, &sig, message, &tree, parents)
        .unwrap();

    repo.find_commit(oid).unwrap()
}
