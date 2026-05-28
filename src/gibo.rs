use log::debug;
use std::io::Read;
use std::process::{Command, ExitStatus, Output, Stdio};
use std::thread;
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
    run_command_with_timeout("git", &args, SUBPROCESS_TIMEOUT)
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
    run_command_with_timeout("gibo", &args, SUBPROCESS_TIMEOUT)
}

fn run_command_with_timeout(
    program: &str,
    args: &[String],
    timeout: Duration,
) -> std::io::Result<Output> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other(format!("{program} stdout was not captured")))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other(format!("{program} stderr was not captured")))?;

    let stdout_handle = thread::spawn(move || read_to_end(stdout));
    let stderr_handle = thread::spawn(move || read_to_end(stderr));
    let deadline = std::time::Instant::now() + timeout;

    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Output {
                status,
                stdout: join_reader(stdout_handle, "stdout")?,
                stderr: join_reader(stderr_handle, "stderr")?,
            });
        }

        let now = std::time::Instant::now();
        if now >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let _ = join_reader(stdout_handle, "stdout");
            let _ = join_reader(stderr_handle, "stderr");
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("{program} timed out after {timeout:?}"),
            ));
        }

        let remaining = deadline.saturating_duration_since(now);
        thread::sleep(remaining.min(Duration::from_millis(10)));
    }
}

fn read_to_end(mut reader: impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn join_reader(
    handle: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    stream: &str,
) -> std::io::Result<Vec<u8>> {
    handle
        .join()
        .map_err(|_| std::io::Error::other(format!("failed to join {stream} reader")))?
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
        let err = validate_gibo_command_output(make_status(0), stdout, "", "C++").unwrap_err();
        assert!(err.to_string().contains("too large"));
    }

    #[test]
    fn test_validate_gibo_list_output_rejects_oversized_stdout() {
        let stdout = "x".repeat(MAX_SUBPROCESS_OUTPUT_BYTES + 1);
        let err = validate_gibo_list_output(make_status(0), stdout, "").unwrap_err();
        assert!(err.to_string().contains("too large"));
    }

    #[test]
    fn test_run_command_with_timeout_captures_stdout() {
        let args = vec!["stdout".to_string()];
        let output = run_command_with_timeout("printf", &args, Duration::from_secs(1)).unwrap();

        assert!(output.status.success());
        assert_eq!(output.stdout, b"stdout");
        assert!(output.stderr.is_empty());
    }

    #[test]
    fn test_run_command_with_timeout_rejects_slow_process() {
        // Spawn `sleep` directly rather than `sh -c "sleep 2"`. On systems
        // where the shell forks for `-c` (instead of exec'ing into the
        // command), the orphaned grandchild keeps the inherited
        // stdout/stderr pipes open after we kill the immediate child,
        // blocking the reader threads until the grandchild finishes
        // naturally.
        let args = vec!["2".to_string()];
        let started = std::time::Instant::now();
        let err = run_command_with_timeout("sleep", &args, Duration::from_millis(50)).unwrap_err();

        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "timeout should return before the child command completes"
        );
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
