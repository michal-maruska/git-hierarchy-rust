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
        base: &Commit<'repo>,
        name: &str,
        file: &str,
    ) -> crate::git_hierarchy::Segment<'repo> {
        let file_path = self.path.join(file);
        fs::write(&file_path, format!("content for {}", name)).unwrap();
        let mut index = self.repo.index().unwrap();
        index.add_path(std::path::Path::new(file)).unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = self.repo.find_tree(tree_id).unwrap();
        let sig = self.repo.signature().unwrap();
        let head_oid = self
            .repo
            .commit(None, &sig, &sig, &format!("commit for {}", name), &tree, &[base])
            .unwrap();

        let base_branch = self.repo.branch(&format!("{}_base", name), base, false).unwrap();
        crate::git_hierarchy::Segment::create(
            &self.repo,
            name,
            base_branch.get(),
            base.id(),
            head_oid,
        )
        .unwrap()
    }

    pub fn create_sample_sum<'repo>(
        &'repo self,
        base: &Commit<'repo>,
        name: &str,
    ) -> crate::git_hierarchy::Sum<'repo> {
        let _seg1 = self.generate_sample_segment(base, &format!("{}_s1", name), &format!("{}_f1.txt", name));
        let _seg2 = self.generate_sample_segment(base, &format!("{}_s2", name), &format!("{}_f2.txt", name));

        let ref1 = self.repo.find_reference(&format!("refs/heads/{}_s1", name)).unwrap();
        let ref2 = self.repo.find_reference(&format!("refs/heads/{}_s2", name)).unwrap();

        let summands = vec![ref1, ref2];
        crate::git_hierarchy::Sum::create(&self.repo, name, summands.iter(), Some(base.clone())).unwrap()
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
