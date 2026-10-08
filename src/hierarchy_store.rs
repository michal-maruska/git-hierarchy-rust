use anyhow::{Context, Result, anyhow, bail};
use git2::{Oid, Repository, Signature};
use crate::git_hierarchy::{GitHierarchy, Segment, Sum, load, segments, sums};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSegment {
    pub name: String,
    pub base: String,
    pub start: String,
    pub head: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSum {
    pub name: String,
    pub head: String,
    pub summands: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HierarchyData {
    pub segments: BTreeMap<String, StoredSegment>,
    pub sums: BTreeMap<String, StoredSum>,
}

impl HierarchyData {
    pub fn collect(repository: &Repository) -> Result<Self> {
        let mut hierarchy = HierarchyData::default();

        let seg_names = segments(repository).context("failed to query segments")?;
        for seg_name in seg_names {
            Segment::check_name_is_valid(&seg_name)?;
            if let Ok(gh) = load(repository, &seg_name) {
                if let GitHierarchy::Segment(seg) = gh {
                    let base_target = seg
                        .base
                        .borrow()
                        .symbolic_target()
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    let start_oid = seg.start().to_string();
                    let head_oid = seg
                        .reference
                        .borrow()
                        .target()
                        .map(|o| o.to_string())
                        .unwrap_or_default();

                    hierarchy.segments.insert(
                        seg_name.clone(),
                        StoredSegment {
                            name: seg_name,
                            base: base_target,
                            start: start_oid,
                            head: head_oid,
                        },
                    );
                }
            }
        }

        let sum_names = sums(repository).context("failed to query sums")?;
        for sum_name in sum_names {
            Segment::check_name_is_valid(&sum_name)?;
            if let Ok(gh) = load(repository, &sum_name) {
                if let GitHierarchy::Sum(sum) = gh {
                    let head_oid = sum
                        .reference
                        .borrow()
                        .target()
                        .map(|o| o.to_string())
                        .unwrap_or_default();
                    let mut summands = Vec::new();
                    for (_idx, _num, summand_ref) in sum.numbered_summands(repository) {
                        if let Some(target) = summand_ref.name() {
                            summands.push(target.to_string());
                        }
                    }
                    hierarchy.sums.insert(
                        sum_name.clone(),
                        StoredSum {
                            name: sum_name,
                            head: head_oid,
                            summands,
                        },
                    );
                }
            }
        }

        Ok(hierarchy)
    }

    pub fn serialize(&self) -> String {
        let mut out = String::new();
        out.push_str("# Git Hierarchy Store\n");
        for seg in self.segments.values() {
            out.push_str(&format!(
                "SEGMENT {} base={} start={} head={}\n",
                seg.name, seg.base, seg.start, seg.head
            ));
        }
        for sum in self.sums.values() {
            out.push_str(&format!(
                "SUM {} head={} summands={}\n",
                sum.name,
                sum.head,
                sum.summands.join(",")
            ));
        }
        out
    }

    pub fn deserialize(input: &str) -> Result<Self> {
        let mut hierarchy = HierarchyData::default();

        for line in input.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            match parts[0] {
                "SEGMENT" => {
                    if parts.len() < 5 {
                        bail!("corrupt SEGMENT line in hierarchy store: '{}'", line);
                    }
                    let name = parts[1].to_string();
                    Segment::check_name_is_valid(&name)?;

                    let mut base = String::new();
                    let mut start = String::new();
                    let mut head = String::new();

                    for p in &parts[2..] {
                        if let Some(v) = p.strip_prefix("base=") {
                            base = v.to_string();
                        } else if let Some(v) = p.strip_prefix("start=") {
                            start = v.to_string();
                        } else if let Some(v) = p.strip_prefix("head=") {
                            head = v.to_string();
                        }
                    }

                    hierarchy.segments.insert(
                        name.clone(),
                        StoredSegment {
                            name,
                            base,
                            start,
                            head,
                        },
                    );
                }
                "SUM" => {
                    if parts.len() < 4 {
                        bail!("corrupt SUM line in hierarchy store: '{}'", line);
                    }
                    let name = parts[1].to_string();
                    Segment::check_name_is_valid(&name)?;

                    let mut head = String::new();
                    let mut summands = Vec::new();

                    for p in &parts[2..] {
                        if let Some(v) = p.strip_prefix("head=") {
                            head = v.to_string();
                        } else if let Some(v) = p.strip_prefix("summands=") {
                            summands = v.split(',').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();
                        }
                    }

                    hierarchy.sums.insert(
                        name.clone(),
                        StoredSum {
                            name,
                            head,
                            summands,
                        },
                    );
                }
                _ => bail!("unknown hierarchy store record type: '{}'", parts[0]),
            }
        }

        Ok(hierarchy)
    }

    pub fn store_to_branch(&self, repository: &Repository, branch_name: &str) -> Result<Oid> {
        Segment::check_name_is_valid(branch_name)?;
        let serialized = self.serialize();
        let blob_oid = repository.blob(serialized.as_bytes())
            .context("failed to write hierarchy blob")?;

        let mut tree_builder = repository.treebuilder(None)
            .context("failed to create tree builder")?;
        tree_builder.insert("hierarchy.txt", blob_oid, 0o100644)
            .context("failed to add hierarchy.txt to tree builder")?;
        let tree_oid = tree_builder.write()
            .context("failed to write hierarchy tree")?;
        let tree = repository.find_tree(tree_oid)?;

        let sig = repository.signature()
            .or_else(|_| Signature::now("git-hierarchy", "git-hierarchy@local"))
            .context("failed to create signature")?;

        let full_ref = format!("refs/heads/{}", branch_name);
        let parent_commit = if let Ok(r) = repository.find_reference(&full_ref) {
            r.peel_to_commit().ok()
        } else {
            None
        };

        let parents: Vec<&git2::Commit<'_>> = parent_commit.as_ref().into_iter().collect();

        let commit_oid = repository.commit(
            Some(&full_ref),
            &sig,
            &sig,
            "Update git hierarchy metadata",
            &tree,
            &parents,
        ).context("failed to commit hierarchy store")?;

        Ok(commit_oid)
    }

    pub fn load_from_branch(repository: &Repository, branch_name: &str) -> Result<Self> {
        Segment::check_name_is_valid(branch_name)?;
        let full_ref = format!("refs/heads/{}", branch_name);
        let reference = repository.find_reference(&full_ref)
            .with_context(|| format!("failed to find hierarchy metadata branch '{}'", branch_name))?;
        let commit = reference.peel_to_commit()?;
        let tree = commit.tree()?;
        let entry = tree.get_name("hierarchy.txt")
            .ok_or_else(|| anyhow!("hierarchy.txt not found in branch '{}'", branch_name))?;
        let blob = repository.find_blob(entry.id())?;
        let content = std::str::from_utf8(blob.content())
            .context("hierarchy.txt content is not valid UTF-8")?;
        Self::deserialize(content)
    }

    pub fn store_to_file(&self, path: &Path) -> Result<()> {
        let serialized = self.serialize();
        fs::write(path, serialized)
            .with_context(|| format!("failed to write hierarchy data to file {:?}", path))
    }

    pub fn load_from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read hierarchy data from file {:?}", path))?;
        Self::deserialize(&content)
    }

    pub fn restore(&self, repository: &Repository, dry_run: bool) -> Result<String> {
        let mut report = String::new();
        report.push_str("=== Hierarchy Restoration Plan ===\n");

        if self.segments.is_empty() && self.sums.is_empty() {
            report.push_str("No stored segments or sums found.\n");
            return Ok(report);
        }

        report.push_str(&format!("Segments ({}):\n", self.segments.len()));
        for seg in self.segments.values() {
            Segment::check_name_is_valid(&seg.name)?;
            let local_gh = load(repository, &seg.name);
            match local_gh {
                Ok(GitHierarchy::Segment(local_seg)) => {
                    let local_start = local_seg.start().to_string();
                    let local_head = local_seg.reference.borrow().target().map(|o| o.to_string()).unwrap_or_default();
                    let local_base = local_seg.base.borrow().symbolic_target().map(|s| s.to_string()).unwrap_or_default();

                    let status = if local_head == seg.head && local_start == seg.start && local_base == seg.base {
                        "Up to date"
                    } else {
                        "Update required"
                    };

                    report.push_str(&format!(
                        "  Segment '{}': {}\n    Base: {} (stored: {})\n    Start: {} (stored: {})\n    Head: {} (stored: {})\n",
                        seg.name, status, local_base, seg.base, local_start, seg.start, local_head, seg.head
                    ));

                    if !dry_run && status == "Update required" {
                        if !seg.base.is_empty() {
                            if let Ok(base_ref) = repository.find_reference(&seg.base) {
                                local_seg.set_base(repository, &base_ref);
                            }
                        }
                        if let Ok(start_oid) = Oid::from_str(&seg.start) {
                            let _ = local_seg.set_start(repository, start_oid, "restore hierarchy");
                        }
                    }
                }
                _ => {
                    report.push_str(&format!(
                        "  Segment '{}': New segment to restore\n    Base: {}\n    Start: {}\n    Head: {}\n",
                        seg.name, seg.base, seg.start, seg.head
                    ));

                    if !dry_run {
                        if let (Ok(base_ref), Ok(start_oid), Ok(head_oid)) = (
                            repository.find_reference(&seg.base),
                            Oid::from_str(&seg.start),
                            Oid::from_str(&seg.head),
                        ) {
                            let _ = Segment::create(repository, &seg.name, &base_ref, start_oid, head_oid);
                        }
                    }
                }
            }
        }

        report.push_str(&format!("Sums ({}):\n", self.sums.len()));
        for sum in self.sums.values() {
            Segment::check_name_is_valid(&sum.name)?;
            let local_gh = load(repository, &sum.name);
            match local_gh {
                Ok(GitHierarchy::Sum(local_sum)) => {
                    let local_head = local_sum.reference.borrow().target().map(|o| o.to_string()).unwrap_or_default();
                    report.push_str(&format!(
                        "  Sum '{}': Head {} (stored: {}), Summands: {}\n",
                        sum.name, local_head, sum.head, sum.summands.join(", ")
                    ));
                }
                _ => {
                    report.push_str(&format!(
                        "  Sum '{}': New sum to restore\n    Head: {}\n    Summands: {}\n",
                        sum.name, sum.head, sum.summands.join(", ")
                    ));

                    if !dry_run {
                        let mut summand_refs = Vec::new();
                        for summand in &sum.summands {
                            if let Ok(r) = repository.find_reference(summand) {
                                summand_refs.push(r);
                            }
                        }
                        if !summand_refs.is_empty() {
                            let hint = Oid::from_str(&sum.head).ok().and_then(|oid| repository.find_commit(oid).ok());
                            let _ = Sum::create(repository, &sum.name, summand_refs.iter(), hint);
                        }
                    }
                }
            }
        }

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{create_commit, TestRepo};

    #[test]
    fn test_serialization_deserialization_roundtrip() {
        let mut hierarchy = HierarchyData::default();
        hierarchy.segments.insert(
            "feature-1".to_string(),
            StoredSegment {
                name: "feature-1".to_string(),
                base: "refs/heads/main".to_string(),
                start: "1111111111111111111111111111111111111111".to_string(),
                head: "2222222222222222222222222222222222222222".to_string(),
            },
        );
        hierarchy.sums.insert(
            "sum-1".to_string(),
            StoredSum {
                name: "sum-1".to_string(),
                head: "3333333333333333333333333333333333333333".to_string(),
                summands: vec!["refs/heads/feature-1".to_string(), "refs/heads/feature-2".to_string()],
            },
        );

        let serialized = hierarchy.serialize();
        let deserialized = HierarchyData::deserialize(&serialized).unwrap();

        assert_eq!(hierarchy, deserialized);
    }

    #[test]
    fn test_deserialize_corrupt_data_returns_error() {
        assert!(HierarchyData::deserialize("SEGMENT name_missing").is_err());
        assert!(HierarchyData::deserialize("SUM name_missing").is_err());
        assert!(HierarchyData::deserialize("UNKNOWN_RECORD name").is_err());
        assert!(HierarchyData::deserialize("SEGMENT -invalid-name base=b start=s head=h").is_err());
    }

    #[test]
    fn test_collect_and_store_to_branch_and_restore() {
        let temp_repo = TestRepo::new();
        let repo = &temp_repo.repo;

        let commit1 = create_commit(repo, "commit 1", &[]);
        let commit2 = create_commit(repo, "commit 2", &[&commit1]);
        let base_branch = repo.branch("main", &commit1, false).unwrap();

        Segment::create(
            repo,
            "feature-a",
            base_branch.get(),
            commit1.id(),
            commit2.id(),
        )
        .unwrap();

        let hierarchy = HierarchyData::collect(repo).unwrap();
        assert!(hierarchy.segments.contains_key("feature-a"));

        let commit_oid = hierarchy.store_to_branch(repo, "_history").unwrap();
        assert_ne!(commit_oid, Oid::zero());

        let loaded_hierarchy = HierarchyData::load_from_branch(repo, "_history").unwrap();
        assert_eq!(hierarchy, loaded_hierarchy);

        let report = loaded_hierarchy.restore(repo, true).unwrap();
        assert!(report.contains("feature-a"));
    }
}
