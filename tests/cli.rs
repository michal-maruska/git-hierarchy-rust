use git_hierarchy::test_utils::TestRepo;
use std::process::Command;

#[test]
fn test_cli_help_git_segment() {
    let output = Command::new(env!("CARGO_BIN_EXE_git-segment"))
        .arg("--help")
        .output()
        .expect("failed to run git-segment");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage:") || stdout.contains("segment"));
}

#[test]
fn test_cli_help_git_sum() {
    let output = Command::new(env!("CARGO_BIN_EXE_git-sum"))
        .arg("--help")
        .output()
        .expect("failed to run git-sum");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage:") || stdout.contains("sum"));
}

#[test]
fn test_cli_help_git_rebase_segment() {
    let output = Command::new(env!("CARGO_BIN_EXE_git-rebase-segment"))
        .arg("--help")
        .output()
        .expect("failed to run git-rebase-segment");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage:"));
}

#[test]
fn test_cli_help_git_rebase_poset() {
    let output = Command::new(env!("CARGO_BIN_EXE_git-rebase-poset"))
        .arg("--help")
        .output()
        .expect("failed to run git-rebase-poset");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage:"));
}

#[test]
fn test_cli_help_git_walk_down() {
    let output = Command::new(env!("CARGO_BIN_EXE_git-walk-down"))
        .arg("--help")
        .output()
        .expect("failed to run git-walk-down");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage:"));
}

#[test]
fn test_cli_define_and_list_segment() {
    let temp_repo = TestRepo::new();
    temp_repo.create_initial_commit();

    // Run git-segment feature main in temp repo directory
    let output = Command::new(env!("CARGO_BIN_EXE_git-segment"))
        .arg("-g")
        .arg(&temp_repo.path)
        .arg("feature")
        .arg("main")
        .output()
        .expect("failed to execute git-segment define");

    assert!(output.status.success(), "Stderr: {}", String::from_utf8_lossy(&output.stderr));

    // Run git-segment -g <path> in temp repo directory to list segments
    let list_output = Command::new(env!("CARGO_BIN_EXE_git-segment"))
        .arg("-g")
        .arg(&temp_repo.path)
        .output()
        .expect("failed to execute git-segment list");

    assert!(list_output.status.success(), "List Stderr: {}\nStdout: {}", String::from_utf8_lossy(&list_output.stderr), String::from_utf8_lossy(&list_output.stdout));
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    assert!(stdout.contains("feature"));
}

#[test]
fn test_cli_define_and_list_sum() {
    let temp_repo = TestRepo::new();
    let commit = temp_repo.create_initial_commit();

    // Create a branch b1 to use as summand
    temp_repo.repo.branch("b1", &commit, false).unwrap();

    // Run git-sum define my-sum b1
    let output = Command::new(env!("CARGO_BIN_EXE_git-sum"))
        .arg("-g")
        .arg(&temp_repo.path)
        .arg("my-sum")
        .arg("b1")
        .output()
        .expect("failed to execute git-sum define");

    assert!(output.status.success(), "Stderr: {}", String::from_utf8_lossy(&output.stderr));

    // Run git-sum -g <path> in temp repo directory to list sums
    let list_output = Command::new(env!("CARGO_BIN_EXE_git-sum"))
        .arg("-g")
        .arg(&temp_repo.path)
        .output()
        .expect("failed to execute git-sum list");

    assert!(list_output.status.success(), "List Stderr: {}\nStdout: {}", String::from_utf8_lossy(&list_output.stderr), String::from_utf8_lossy(&list_output.stdout));
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    assert!(stdout.contains("my-sum"));
}

#[test]
fn test_cli_rebase_poset_corrupt_marker_file() {
    let temp_repo = TestRepo::new();
    temp_repo.create_initial_commit();

    // Create a corrupt marker file in commondir
    let marker_path = temp_repo.repo.commondir().join(".segment-cherry-pick");
    std::fs::write(&marker_path, "feature\ncorrupt_data\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_git-rebase-poset"))
        .arg("-g")
        .arg(&temp_repo.path)
        .output()
        .expect("failed to execute git-rebase-poset");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Error reading rebase state"));
}

#[test]
fn test_cli_sum_rejects_invalid_summand_name() {
    let temp_repo = TestRepo::new();
    temp_repo.create_initial_commit();

    let output = Command::new(env!("CARGO_BIN_EXE_git-sum"))
        .arg("-g")
        .arg(&temp_repo.path)
        .arg("--define")
        .arg("--")
        .arg("my-sum")
        .arg("-invalid-summand")
        .output()
        .expect("failed to execute git-sum define");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid reference name: -invalid-summand"), "Stderr was: {}", stderr);
}

#[test]
fn test_cli_git_segment_nonexistent() {
    let temp_repo = TestRepo::new();
    temp_repo.create_initial_commit();

    let output = Command::new(env!("CARGO_BIN_EXE_git-segment"))
        .arg("-g")
        .arg(&temp_repo.path)
        .arg("restart")
        .arg("nonexistent-segment")
        .arg("main")
        .output()
        .expect("failed to execute git-segment");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no reference found for shorthand 'nonexistent-segment'"), "Stderr was: {}", stderr);
}

#[test]
fn test_cli_segment_rejects_invalid_name() {
    let temp_repo = TestRepo::new();
    temp_repo.create_initial_commit();

    let output = Command::new(env!("CARGO_BIN_EXE_git-segment"))
        .arg("-g")
        .arg(&temp_repo.path)
        .arg("--define")
        .arg("--")
        .arg("feature")
        .arg("-invalid-base")
        .output()
        .expect("failed to execute git-segment define");

    assert!(!output.status.success());
}

#[test]
fn test_cli_walk_down_success() {
    let temp_repo = TestRepo::new();
    temp_repo.create_initial_commit();

    let seg_output = Command::new(env!("CARGO_BIN_EXE_git-segment"))
        .arg("-g")
        .arg(&temp_repo.path)
        .arg("feature")
        .arg("main")
        .output()
        .expect("failed to define segment");
    assert!(seg_output.status.success());

    let output = Command::new(env!("CARGO_BIN_EXE_git-walk-down"))
        .arg("-g")
        .arg(&temp_repo.path)
        .arg("feature")
        .output()
        .expect("failed to execute git-walk-down");

    assert!(output.status.success(), "Stderr was: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("feature"));
}

#[test]
fn test_cli_walk_down_rejects_invalid_name() {
    let temp_repo = TestRepo::new();
    temp_repo.create_initial_commit();

    let output = Command::new(env!("CARGO_BIN_EXE_git-walk-down"))
        .arg("-g")
        .arg(&temp_repo.path)
        .arg("--")
        .arg("-invalid-root")
        .output()
        .expect("failed to execute git-walk-down");

    assert!(!output.status.success());
}

#[test]
fn test_cli_segment_update_rejects_invalid_base_name() {
    let temp_repo = TestRepo::new();
    temp_repo.create_initial_commit();

    let output = Command::new(env!("CARGO_BIN_EXE_git-segment"))
        .arg("-g")
        .arg(&temp_repo.path)
        .arg("update")
        .arg("--")
        .arg("feature")
        .arg("-invalid-base")
        .output()
        .expect("failed to execute git-segment update");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid reference name: -invalid-base"), "Stderr was: {}", stderr);
}

#[test]
fn test_cli_rebase_continuation_after_conflict() {
    let temp_repo = TestRepo::new();

    // 1. Create base commit on main with file1.txt: "1\n2\n3\n"
    let file1_path = temp_repo.path.join("file1.txt");
    std::fs::write(&file1_path, "1\n2\n3\n").unwrap();
    let mut index = temp_repo.repo.index().unwrap();
    index.add_path(std::path::Path::new("file1.txt")).unwrap();
    index.write().unwrap();
    let base_commit = temp_repo.create_commit("initial commit", &[]);
    temp_repo.repo.branch("main", &base_commit, true).unwrap();

    // 2. Define segment 'feature' with base 'main'
    let output = Command::new(env!("CARGO_BIN_EXE_git-segment"))
        .arg("-g")
        .arg(&temp_repo.path)
        .arg("feature")
        .arg("main")
        .output()
        .expect("failed to execute git-segment define");
    assert!(output.status.success(), "git-segment define failed: {}", String::from_utf8_lossy(&output.stderr));

    // 3. Create feature segment commits on feature branch:
    temp_repo.repo.set_head("refs/heads/feature").unwrap();
    temp_repo.repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();

    // change 1: file1.txt modified to "1\nA\n2\n3\n"
    std::fs::write(&file1_path, "1\nA\n2\n3\n").unwrap();
    let mut index = temp_repo.repo.index().unwrap();
    index.add_path(std::path::Path::new("file1.txt")).unwrap();
    index.write().unwrap();
    let change1_commit = temp_repo.create_commit("change 1", &[&base_commit]);

    // change 3: file2.txt added with "file 2 content\n" (different file)
    let file2_path = temp_repo.path.join("file2.txt");
    std::fs::write(&file2_path, "file 2 content\n").unwrap();
    let mut index = temp_repo.repo.index().unwrap();
    index.add_path(std::path::Path::new("file2.txt")).unwrap();
    index.write().unwrap();
    let change3_commit = temp_repo.create_commit("change 3 on different file", &[&change1_commit]);

    temp_repo.repo.reference("refs/heads/feature", change3_commit.id(), true, "update feature").unwrap();

    // 4. Update main to change 2 (file1.txt modified to "1\n2 modified\n3\n")
    temp_repo.repo.set_head("refs/heads/main").unwrap();
    temp_repo.repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();

    std::fs::write(&file1_path, "1\n2 modified\n3\n").unwrap();
    let mut index = temp_repo.repo.index().unwrap();
    index.add_path(std::path::Path::new("file1.txt")).unwrap();
    index.write().unwrap();
    let change2_commit = temp_repo.create_commit("change 2", &[&base_commit]);
    temp_repo.repo.reference("refs/heads/main", change2_commit.id(), true, "update main").unwrap();

    // 5. Run git-rebase-poset on feature
    let output = Command::new(env!("CARGO_BIN_EXE_git-rebase-poset"))
        .arg("-g")
        .arg(&temp_repo.path)
        .arg("-f")
        .arg("feature")
        .output()
        .expect("failed to execute git-rebase-poset");

    assert!(!output.status.success(), "git-rebase-poset should have failed due to conflicts");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("conflicts detected"), "Stderr was: {}", stderr);

    // 6. Resolve conflict in file1.txt to "1\nA\n2 modified\n3\n" and stage
    std::fs::write(&file1_path, "1\nA\n2 modified\n3\n").unwrap();
    let mut index = temp_repo.repo.index().unwrap();
    index.add_path(std::path::Path::new("file1.txt")).unwrap();
    index.write().unwrap();

    // 7. Invoke continuation: git-rebase-poset -c feature
    let cont_output = Command::new(env!("CARGO_BIN_EXE_git-rebase-poset"))
        .arg("-g")
        .arg(&temp_repo.path)
        .arg("-f")
        .arg("-c")
        .arg("feature")
        .output()
        .expect("failed to execute git-rebase-poset --continue");

    assert!(cont_output.status.success(), "git-rebase-poset --continue failed: {}\nStdout: {}", String::from_utf8_lossy(&cont_output.stderr), String::from_utf8_lossy(&cont_output.stdout));

    // 8. Verify resolved contents and final state on feature branch
    temp_repo.repo.set_head("refs/heads/feature").unwrap();
    temp_repo.repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();

    let content1 = std::fs::read_to_string(&file1_path).unwrap();
    assert_eq!(content1, "1\nA\n2 modified\n3\n");

    let content2 = std::fs::read_to_string(&file2_path).unwrap();
    assert_eq!(content2, "file 2 content\n");
}

#[test]
fn test_cli_rebase_cherrypick_failed_uncommitted_changes_continuation() {
    let temp_repo = TestRepo::new();

    // 1. Base commit on main with file1.txt
    let file1_path = temp_repo.path.join("file1.txt");
    std::fs::write(&file1_path, "base content\n").unwrap();
    let mut index = temp_repo.repo.index().unwrap();
    index.add_path(std::path::Path::new("file1.txt")).unwrap();
    index.write().unwrap();
    let base_commit = temp_repo.create_commit("initial commit", &[]);
    temp_repo.repo.branch("main", &base_commit, true).unwrap();
    temp_repo.repo.set_head("refs/heads/main").unwrap();

    // 2. Define and checkout segment 'feature' with base 'main' using 'create'
    let output = Command::new(env!("CARGO_BIN_EXE_git-segment"))
        .arg("-g")
        .arg(&temp_repo.path)
        .arg("create")
        .arg("feature")
        .arg("main")
        .output()
        .expect("failed to execute git-segment create");
    assert!(output.status.success(), "git-segment create failed: {}", String::from_utf8_lossy(&output.stderr));

    // 3. Create feature segment commit on feature branch:
    let file2_path = temp_repo.path.join("file2.txt");
    std::fs::write(&file2_path, "feature file2 content\n").unwrap();
    let mut index = temp_repo.repo.index().unwrap();
    index.add_path(std::path::Path::new("file2.txt")).unwrap();
    index.write().unwrap();
    let feature_commit = temp_repo.create_commit("feature change", &[&base_commit]);
    temp_repo.repo.reference("refs/heads/feature", feature_commit.id(), true, "update feature").unwrap();

    // 4. Update main branch with another commit
    temp_repo.repo.set_head("refs/heads/main").unwrap();
    temp_repo.repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();

    let file3_path = temp_repo.path.join("file3.txt");
    std::fs::write(&file3_path, "main file3 content\n").unwrap();
    let mut index = temp_repo.repo.index().unwrap();
    index.add_path(std::path::Path::new("file3.txt")).unwrap();
    index.write().unwrap();
    let main_commit = temp_repo.create_commit("main change", &[&base_commit]);
    temp_repo.repo.reference("refs/heads/main", main_commit.id(), true, "update main").unwrap();

    // 5. Create uncommitted change on file2.txt in worktree so cherry-pick will fail
    std::fs::write(&file2_path, "uncommitted worktree content\n").unwrap();

    // 6. Run git-rebase-poset on feature -> expect failure because uncommitted file2.txt would be overwritten
    let output = Command::new(env!("CARGO_BIN_EXE_git-rebase-poset"))
        .arg("-g")
        .arg(&temp_repo.path)
        .arg("-f")
        .arg("feature")
        .output()
        .expect("failed to execute git-rebase-poset");

    assert!(!output.status.success(), "git-rebase-poset should fail due to uncommitted change");

    // Check marker file contents
    let marker_path = temp_repo.repo.commondir().join(".segment-cherry-pick");
    let marker_content = std::fs::read_to_string(&marker_path).unwrap();
    assert_eq!(marker_content, format!("feature\n0\n{}\n", feature_commit.id()));

    // 7. Clean uncommitted change in worktree
    std::fs::remove_file(&file2_path).unwrap();

    // 8. Run git-rebase-poset --continue
    let cont_output = Command::new(env!("CARGO_BIN_EXE_git-rebase-poset"))
        .arg("-g")
        .arg(&temp_repo.path)
        .arg("-f")
        .arg("-c")
        .arg("feature")
        .output()
        .expect("failed to execute git-rebase-poset --continue");

    assert!(cont_output.status.success(), "git-rebase-poset --continue failed: {}\nStdout: {}", String::from_utf8_lossy(&cont_output.stderr), String::from_utf8_lossy(&cont_output.stdout));

    // 9. Verify feature branch contains file2.txt with "feature file2 content\n"
    temp_repo.repo.set_head("refs/heads/feature").unwrap();
    temp_repo.repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();

    assert!(file2_path.exists(), "file2.txt should exist on feature branch after rebase continue!");
    let content = std::fs::read_to_string(&file2_path).unwrap();
    assert_eq!(content, "feature file2 content\n");
}
