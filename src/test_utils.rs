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

    pub fn generate_sample_segment<'repo>(
        &'repo self,
        base_branch_name: &str,
        name: &str,
        file: &str,
    ) -> crate::git_hierarchy::Segment<'repo> {
        let base_ref = self.repo.resolve_reference_from_short_name(base_branch_name).unwrap();
        let base_commit = base_ref.peel_to_commit().unwrap();
        let start_oid = base_commit.id();

        let file_path = self.path.join(file);
        fs::write(&file_path, format!("content for {}", name)).unwrap();
        let mut index = self.repo.index().unwrap();
        index.add_path(std::path::Path::new(file)).unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = self.repo.find_tree(tree_id).unwrap();
        let sig = self.repo.signature().unwrap();
        let head_oid = self
            .repo
            .commit(None, &sig, &sig, &format!("commit for {}", name), &tree, &[&base_commit])
            .unwrap();

        crate::git_hierarchy::Segment::create(
            &self.repo,
            name,
            &base_ref,
            start_oid,
            head_oid,
        )
        .unwrap()
    }

    pub fn create_sample_sum<'repo>(
        &'repo self,
        name: &str,
        summand_branches: &[&str],
    ) -> crate::git_hierarchy::Sum<'repo> {
        let refs: Vec<_> = summand_branches
            .iter()
            .map(|b| self.repo.resolve_reference_from_short_name(b).unwrap())
            .collect();

        crate::git_hierarchy::Sum::create(&self.repo, name, refs.iter(), None).unwrap()
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
