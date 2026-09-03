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
