use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_gitignore-in");

#[test]
fn help_flag_exits_successfully() {
    let output = Command::new(BIN).arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Manage .gitignore files"));
}

#[test]
fn version_flag_exits_successfully() {
    let output = Command::new(BIN).arg("--version").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("gitignore.in"));
}

#[test]
fn build_in_empty_dir_creates_gitignore_in() {
    let tmp = tempfile::tempdir().unwrap();
    let output = Command::new(BIN).current_dir(tmp.path()).output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(tmp.path().join(".gitignore.in").exists());
    assert!(tmp.path().join(".gitignore").exists());
    let gitignore_in = std::fs::read_to_string(tmp.path().join(".gitignore.in")).unwrap();
    assert!(gitignore_in.contains("# See https://gitignore.in/"));
}

#[test]
fn build_with_existing_gitignore_in_produces_gitignore() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join(".gitignore.in"),
        "# See https://gitignore.in/\n# Edit this file and run `gitignore.in` to rebuild .gitignore\n\necho '*.log'\n",
    )
    .unwrap();
    let output = Command::new(BIN).current_dir(tmp.path()).output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let gitignore = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
    assert!(gitignore.contains("*.log"));
}

#[test]
fn default_build_reads_boilerplates_ref_env() {
    let tmp = tempfile::tempdir().unwrap();
    let output = Command::new(BIN)
        .env(
            "GITIGNORE_IN_BOILERPLATES_REF",
            "gitignore-in-test-missing-ref",
        )
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Cannot resolve \"gitignore-in-test-missing-ref\""),
        "stderr should show that the default build read the ref env var: {stderr}"
    );
    assert!(!tmp.path().join(".gitignore").exists());
}

#[test]
fn build_rejects_multiple_template_names_on_one_line() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join(".gitignore.in"),
        "# See https://gitignore.in/\n# Edit this file and run `gitignore.in` to rebuild .gitignore\n\ngibo dump Rust macOS\n",
    )
    .unwrap();

    let output = Command::new(BIN).current_dir(tmp.path()).output().unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stdout.is_empty(),
        "stdout should stay reserved for data output: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("gibo dump expects one template per line"),
        "stderr should explain the invalid template line: {stderr}"
    );
    assert!(!tmp.path().join(".gitignore").exists());
}
