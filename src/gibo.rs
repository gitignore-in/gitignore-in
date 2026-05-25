use log::debug;
use std::process::{ExitStatus, Output};
use std::sync::mpsc;
use std::time::Duration;

const SUBPROCESS_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_SUBPROCESS_OUTPUT_BYTES: usize = 10 * 1024 * 1024;

pub fn gibo_root() -> std::io::Result<String> {
    let output = run_gibo_with_timeout(&["root"])?;
    let stdout = String::from_utf8(output.stdout)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let stderr = String::from_utf8(output.stderr)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if !output.status.success() {
        let code = output
            .status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "<signal>".to_string());
        return Err(std::io::Error::other(format!(
            "gibo root failed: exit={code} stderr={stderr}"
        )));
    }
    let root = stdout.trim().to_string();
    if root.is_empty() {
        return Err(std::io::Error::other(
            "gibo root returned empty output; boilerplates database not initialised",
        ));
    }
    Ok(root)
}

fn run_git_with_timeout(args: &[&str]) -> std::io::Result<Output> {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = std::process::Command::new("git").args(&args).output();
        let _ = tx.send(result);
    });
    match rx.recv_timeout(SUBPROCESS_TIMEOUT) {
        Ok(result) => result,
        Err(_) => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("git timed out after {}s", SUBPROCESS_TIMEOUT.as_secs()),
        )),
    }
}

pub fn pin_boilerplates(ref_spec: &str) -> std::io::Result<()> {
    let root = gibo_root()?;
    // Resolve to a commit SHA so we always get a detached HEAD regardless of
    // whether ref_spec is a branch, tag, or full SHA.
    let resolve_output = run_git_with_timeout(&[
        "-C",
        &root,
        "rev-parse",
        "--verify",
        &format!("{ref_spec}^{{commit}}"),
    ])?;
    if !resolve_output.status.success() {
        let stderr = String::from_utf8_lossy(&resolve_output.stderr);
        return Err(std::io::Error::other(format!(
            "Cannot resolve {ref_spec:?} in boilerplates at {root}: {stderr}"
        )));
    }
    let sha = String::from_utf8(resolve_output.stdout)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let sha = sha.trim();
    let checkout_output = run_git_with_timeout(&["-C", &root, "checkout", "--detach", sha])?;
    if !checkout_output.status.success() {
        let stderr = String::from_utf8_lossy(&checkout_output.stderr);
        return Err(std::io::Error::other(format!(
            "Failed to pin boilerplates to {sha} ({ref_spec:?}) in {root}: {stderr}"
        )));
    }
    Ok(())
}

fn run_gibo_with_timeout(args: &[&str]) -> std::io::Result<Output> {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = std::process::Command::new("gibo").args(&args).output();
        let _ = tx.send(result);
    });
    match rx.recv_timeout(SUBPROCESS_TIMEOUT) {
        Ok(result) => result,
        Err(_) => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("gibo timed out after {}s", SUBPROCESS_TIMEOUT.as_secs()),
        )),
    }
}

fn validate_gibo_command_output(
    status: ExitStatus,
    stdout: String,
    stderr: &str,
    target: &str,
) -> std::io::Result<String> {
    if status.success() {
        if stdout.is_empty() {
            return Err(std::io::Error::other(format!(
                "Failed to get {target} from gibo: empty stdout (stderr={stderr})"
            )));
        }
        if stdout.len() > MAX_SUBPROCESS_OUTPUT_BYTES {
            return Err(std::io::Error::other(format!(
                "Failed to get {target} from gibo: output too large ({} bytes, max {MAX_SUBPROCESS_OUTPUT_BYTES})",
                stdout.len()
            )));
        }
        return Ok(stdout);
    }
    let code = status
        .code()
        .map(|c| c.to_string())
        .unwrap_or_else(|| "<signal>".to_string());
    Err(std::io::Error::other(format!(
        "Failed to get {target} from gibo: exit={code} stderr={stderr}"
    )))
}

fn validate_gibo_list_output(
    status: ExitStatus,
    stdout: String,
    stderr: &str,
) -> std::io::Result<Vec<String>> {
    if !status.success() {
        let code = status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "<signal>".to_string());
        return Err(std::io::Error::other(format!(
            "Failed to list templates from gibo: exit={code} stderr={stderr}"
        )));
    }
    if stdout.len() > MAX_SUBPROCESS_OUTPUT_BYTES {
        return Err(std::io::Error::other(format!(
            "Failed to list templates from gibo: output too large ({} bytes, max {MAX_SUBPROCESS_OUTPUT_BYTES})",
            stdout.len()
        )));
    }
    Ok(stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect())
}

pub fn gibo_command(target: &str) -> std::io::Result<String> {
    let started = std::time::Instant::now();
    let output = run_gibo_with_timeout(&["dump", target])?;
    let elapsed_ms = started.elapsed().as_millis();
    let code = output
        .status
        .code()
        .map(|c| c.to_string())
        .unwrap_or_else(|| "<signal>".to_string());
    debug!("gibo dump {target} -> exit={code} ({elapsed_ms:.0}ms)");

    let stdout = match String::from_utf8(output.stdout) {
        Ok(it) => it,
        Err(err) => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, err)),
    };
    let stderr = match String::from_utf8(output.stderr) {
        Ok(it) => it,
        Err(err) => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, err)),
    };
    validate_gibo_command_output(output.status, stdout, &stderr, target)
}

pub fn gibo_list() -> std::io::Result<Vec<String>> {
    let started = std::time::Instant::now();
    let output = run_gibo_with_timeout(&["list"])?;
    let elapsed_ms = started.elapsed().as_millis();
    let code = output
        .status
        .code()
        .map(|c| c.to_string())
        .unwrap_or_else(|| "<signal>".to_string());
    debug!("gibo list -> exit={code} ({elapsed_ms:.0}ms)");
    let stdout = match String::from_utf8(output.stdout) {
        Ok(it) => it,
        Err(err) => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, err)),
    };
    let stderr = match String::from_utf8(output.stderr) {
        Ok(it) => it,
        Err(err) => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, err)),
    };
    validate_gibo_list_output(output.status, stdout, &stderr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    fn make_status(code: i32) -> ExitStatus {
        // unix では exit code は (code << 8) で表現される。
        ExitStatus::from_raw(code << 8)
    }

    #[test]
    fn test_validate_gibo_command_output_ok() {
        let stdout = "### Generated by gibo ###\nfoo\n".to_string();
        let result =
            validate_gibo_command_output(make_status(0), stdout.clone(), "", "C++").unwrap();
        assert_eq!(result, stdout);
    }

    #[test]
    fn test_validate_gibo_command_output_rejects_non_zero_exit() {
        let err = validate_gibo_command_output(
            make_status(1),
            String::new(),
            "gibo: failed to clone repository",
            "C++",
        )
        .unwrap_err();
        assert!(err.to_string().contains("exit=1"));
        assert!(err.to_string().contains("failed to clone"));
    }

    #[test]
    fn test_validate_gibo_command_output_rejects_non_zero_with_stdout() {
        // exit 非ゼロでも部分的 stdout が出ているケース。garbage を
        // `.gitignore` に書き込まないように reject する。
        let err = validate_gibo_command_output(
            make_status(2),
            "partial output\n".to_string(),
            "boilerplate not found".to_string().as_str(),
            "C++",
        )
        .unwrap_err();
        assert!(err.to_string().contains("exit=2"));
    }

    #[test]
    fn test_validate_gibo_command_output_rejects_zero_exit_empty_stdout() {
        // exit 0 でも stdout が空なら空 `.gitignore` 書き込みを防ぐため reject。
        let err =
            validate_gibo_command_output(make_status(0), String::new(), "warn", "C++").unwrap_err();
        assert!(err.to_string().contains("empty stdout"));
    }

    #[test]
    fn test_validate_gibo_list_output_ok() {
        let stdout = "C++\nRust\nPython\n".to_string();
        let result = validate_gibo_list_output(make_status(0), stdout, "").unwrap();
        assert_eq!(result, vec!["C++", "Rust", "Python"]);
    }

    #[test]
    fn test_validate_gibo_list_output_rejects_non_zero_exit() {
        let err =
            validate_gibo_list_output(make_status(127), String::new(), "gibo: command failed")
                .unwrap_err();
        assert!(err.to_string().contains("exit=127"));
    }

    #[test]
    fn test_validate_gibo_command_output_rejects_oversized_stdout() {
        let stdout = "x".repeat(MAX_SUBPROCESS_OUTPUT_BYTES + 1);
        let err =
            validate_gibo_command_output(make_status(0), stdout, "", "C++").unwrap_err();
        assert!(err.to_string().contains("too large"));
    }

    #[test]
    fn test_validate_gibo_list_output_rejects_oversized_stdout() {
        let stdout = "x".repeat(MAX_SUBPROCESS_OUTPUT_BYTES + 1);
        let err = validate_gibo_list_output(make_status(0), stdout, "").unwrap_err();
        assert!(err.to_string().contains("too large"));
    }

    #[test]
    fn test_gibo_command() {
        let result = gibo_command("C++");
        let result = match result {
            Ok(result) => result,
            Err(e) => {
                eprintln!("{e}");
                unreachable!();
            }
        };
        assert!(result.contains("Generated by gibo"));
        assert!(result.contains("C++"));
    }

    #[test]
    fn test_gibo_command_fail() {
        let result = gibo_command("unknown-language");
        assert!(result.is_err());
    }
}
