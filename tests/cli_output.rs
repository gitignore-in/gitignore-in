use std::process::Command;

use tempfile::TempDir;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_gitignore-in")
}

#[test]
fn build_progress_is_suppressed_when_stderr_is_not_a_tty() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");

    let output = Command::new(binary())
        .current_dir(temp_dir.path())
        .output()
        .expect("failed to run gitignore-in");

    assert!(
        output.status.success(),
        "status: {:?}",
        output.status.code()
    );
    assert!(
        output.stdout.is_empty(),
        "stdout should not contain progress output: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        output.stderr.is_empty(),
        "captured stderr should not contain progress output: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn quiet_suppresses_progress_output() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");

    let output = Command::new(binary())
        .arg("--quiet")
        .current_dir(temp_dir.path())
        .output()
        .expect("failed to run gitignore-in");

    assert!(
        output.status.success(),
        "status: {:?}",
        output.status.code()
    );
    assert!(
        output.stdout.is_empty(),
        "stdout should not contain progress output: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        output.stderr.is_empty(),
        "stderr should not contain progress output: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn invalid_user_input_goes_to_stderr_once() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");

    let output = Command::new(binary())
        .args(["add"])
        .current_dir(temp_dir.path())
        .output()
        .expect("failed to run gitignore-in");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stdout.is_empty(),
        "stdout should stay reserved for data output: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr
            .matches("At least one template name is required")
            .count(),
        1,
        "stderr should contain one user-facing error: {stderr}"
    );
    assert!(!stderr.contains("InvalidInput"));
}

#[test]
fn search_no_match_exits_with_general_error_not_usage_error() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");

    let output = Command::new(binary())
        .args(["search", "zzz-this-cannot-possibly-match-any-template"])
        .current_dir(temp_dir.path())
        .output()
        .expect("failed to run gitignore-in");

    assert_eq!(
        output.status.code(),
        Some(1),
        "no-match search should exit 1 (general error), not 2 (usage error)"
    );
    assert!(
        output.stdout.is_empty(),
        "stdout should stay reserved for data output: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No templates matched"),
        "stderr should explain the no-match result: {stderr}"
    );
}
