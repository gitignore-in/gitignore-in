use crate::gi::sanitize_target;
use log::debug;
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitStatus, Output, Stdio};
use std::thread;
use std::time::Duration;

const SUBPROCESS_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_SUBPROCESS_OUTPUT_BYTES: usize = 10 * 1024 * 1024;
const MAX_SUBPROCESS_STDERR_BYTES: usize = 4 * 1024;

fn truncate_stderr(s: &str) -> String {
    let sanitized: String = s.chars().filter(|c| !c.is_control()).collect();
    if sanitized.len() <= MAX_SUBPROCESS_STDERR_BYTES {
        return sanitized;
    }
    let mut end = MAX_SUBPROCESS_STDERR_BYTES;
    while !sanitized.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{} ...[{} bytes truncated]",
        &sanitized[..end],
        sanitized.len() - end
    )
}

fn strip_control_chars(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

pub fn gibo_root() -> std::io::Result<String> {
    let output = run_gibo_with_timeout(&["root"])?;
    let code = output
        .status
        .code()
        .map(|c| c.to_string())
        .unwrap_or_else(|| "<signal>".to_string());
    let stdout = String::from_utf8(output.stdout).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("gibo root: stdout is not valid UTF-8 (exit={code}): {e}"),
        )
    })?;
    let stderr = String::from_utf8(output.stderr).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("gibo root: stderr is not valid UTF-8 (exit={code}): {e}"),
        )
    })?;
    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "gibo root failed: exit={code} stderr={}",
            truncate_stderr(&stderr)
        )));
    }
    let root = strip_control_chars(stdout.trim());
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
            "Cannot resolve {ref_spec:?} in boilerplates at {root}: {}",
            truncate_stderr(&stderr)
        )));
    }
    let sha = String::from_utf8(resolve_output.stdout)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let sha = sha.trim();
    let checkout_output = run_git_with_timeout(&["-C", &root, "checkout", "--detach", sha])?;
    if !checkout_output.status.success() {
        let stderr = String::from_utf8_lossy(&checkout_output.stderr);
        return Err(std::io::Error::other(format!(
            "Failed to pin boilerplates to {sha} ({ref_spec:?}) in {root}: {}",
            truncate_stderr(&stderr)
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
        .process_group(0)
        .spawn()?;
    let process_group_id = match libc::pid_t::try_from(child.id()) {
        Ok(pid) => pid,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::other(format!(
                "{program} pid is out of range"
            )));
        }
    };

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other(format!("{program} stdout was not captured")))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other(format!("{program} stderr was not captured")))?;

    let mut stdout_handle = Some(thread::spawn(move || read_to_end(stdout)));
    let mut stderr_handle = Some(thread::spawn(move || read_to_end(stderr)));
    let mut stdout_bytes = None;
    let mut stderr_bytes = None;
    let deadline = std::time::Instant::now() + timeout;

    loop {
        if let Err(error) = collect_finished_reader(&mut stdout_handle, &mut stdout_bytes, "stdout")
        {
            let _ = terminate_child(&mut child, process_group_id);
            return Err(error);
        }
        if let Err(error) = collect_finished_reader(&mut stderr_handle, &mut stderr_bytes, "stderr")
        {
            let _ = terminate_child(&mut child, process_group_id);
            return Err(error);
        }

        if let Some(status) = child.try_wait()? {
            return Ok(Output {
                status,
                stdout: finish_reader(stdout_handle, stdout_bytes, "stdout")?,
                stderr: finish_reader(stderr_handle, stderr_bytes, "stderr")?,
            });
        }

        let now = std::time::Instant::now();
        if now >= deadline {
            let _ = terminate_child(&mut child, process_group_id);
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("{program} timed out after {timeout:?}"),
            ));
        }

        let remaining = deadline.saturating_duration_since(now);
        thread::sleep(remaining.min(Duration::from_millis(10)));
    }
}

fn terminate_child(
    child: &mut std::process::Child,
    process_group_id: libc::pid_t,
) -> std::io::Result<()> {
    let process_group_result = kill_process_group(process_group_id);
    let child_kill_result = child.kill();
    let wait_result = child.wait().map(|_| ());
    process_group_result.or(child_kill_result).or(wait_result)
}

fn kill_process_group(process_group_id: libc::pid_t) -> std::io::Result<()> {
    if process_group_id <= 0 {
        return Err(std::io::Error::other("process group id must be positive"));
    }

    let result = unsafe { libc::kill(-process_group_id, libc::SIGKILL) };
    if result == 0 {
        return Ok(());
    }

    let err = std::io::Error::last_os_error();
    if err.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(err)
}

fn read_to_end(reader: impl Read) -> std::io::Result<Vec<u8>> {
    let limit = MAX_SUBPROCESS_OUTPUT_BYTES as u64 + 1;
    let mut bytes = Vec::new();
    reader.take(limit).read_to_end(&mut bytes)?;
    if bytes.len() as u64 >= limit {
        return Err(std::io::Error::other(format!(
            "subprocess output exceeds {MAX_SUBPROCESS_OUTPUT_BYTES} bytes"
        )));
    }
    Ok(bytes)
}

fn collect_finished_reader(
    handle: &mut Option<thread::JoinHandle<std::io::Result<Vec<u8>>>>,
    bytes: &mut Option<Vec<u8>>,
    stream: &str,
) -> std::io::Result<()> {
    let Some(handle_ref) = handle.as_ref() else {
        return Ok(());
    };
    if !handle_ref.is_finished() {
        return Ok(());
    }

    let joined = join_reader(
        handle
            .take()
            .ok_or_else(|| std::io::Error::other(format!("{stream} reader missing")))?,
        stream,
    )?;
    *bytes = Some(joined);
    Ok(())
}

fn finish_reader(
    handle: Option<thread::JoinHandle<std::io::Result<Vec<u8>>>>,
    bytes: Option<Vec<u8>>,
    stream: &str,
) -> std::io::Result<Vec<u8>> {
    if let Some(bytes) = bytes {
        return Ok(bytes);
    }
    let handle = handle.ok_or_else(|| std::io::Error::other(format!("{stream} reader missing")))?;
    join_reader(handle, stream)
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
    let target = sanitize_target(target);
    if status.success() {
        if stdout.is_empty() {
            return Err(std::io::Error::other(format!(
                "Failed to get {target} from gibo: empty stdout (stderr={})",
                truncate_stderr(stderr)
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
        "Failed to get {target} from gibo: exit={code} stderr={}",
        truncate_stderr(stderr)
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
            "Failed to list templates from gibo: exit={code} stderr={}",
            truncate_stderr(stderr)
        )));
    }
    if stdout.len() > MAX_SUBPROCESS_OUTPUT_BYTES {
        return Err(std::io::Error::other(format!(
            "Failed to list templates from gibo: output too large ({} bytes, max {MAX_SUBPROCESS_OUTPUT_BYTES})",
            stdout.len()
        )));
    }
    let templates: Vec<String> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect();
    if templates.is_empty() {
        return Err(std::io::Error::other(
            "Failed to list templates from gibo: empty output (boilerplate DB may be uninitialized; run `gibo update`)",
        ));
    }
    Ok(templates)
}

pub fn gibo_command(target: &str) -> std::io::Result<String> {
    let target = sanitize_target(target);
    let started = std::time::Instant::now();
    let output = run_gibo_with_timeout(&["dump", &target])?;
    let elapsed_ms = started.elapsed().as_millis();
    let code = output
        .status
        .code()
        .map(|c| c.to_string())
        .unwrap_or_else(|| "<signal>".to_string());
    debug!("gibo dump {target} -> exit={code} ({elapsed_ms:.0}ms)");

    let stdout = match String::from_utf8(output.stdout) {
        Ok(it) => it,
        Err(err) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("gibo dump {target}: stdout is not valid UTF-8 (exit={code}): {err}"),
            ))
        }
    };
    let stderr = match String::from_utf8(output.stderr) {
        Ok(it) => it,
        Err(err) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("gibo dump {target}: stderr is not valid UTF-8 (exit={code}): {err}"),
            ))
        }
    };
    validate_gibo_command_output(output.status, stdout, &stderr, &target)
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
        Err(err) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("gibo list: stdout is not valid UTF-8 (exit={code}): {err}"),
            ))
        }
    };
    let stderr = match String::from_utf8(output.stderr) {
        Ok(it) => it,
        Err(err) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("gibo list: stderr is not valid UTF-8 (exit={code}): {err}"),
            ))
        }
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
    fn test_strip_control_chars_removes_esc_from_ansi_sequences() {
        // ANSI sequences start with ESC (0x1b, a control char). strip_control_chars
        // removes the ESC byte, leaving the non-control remnant "[32m...[0m" harmless
        // (no ESC prefix → no terminal interpretation).
        let raw = "\x1b[32m/home/user/boilerplates\x1b[0m";
        assert_eq!(strip_control_chars(raw), "[32m/home/user/boilerplates[0m");
    }

    #[test]
    fn test_strip_control_chars_passthrough_clean_path() {
        let path = "/home/user/boilerplates";
        assert_eq!(strip_control_chars(path), path);
    }

    #[test]
    fn test_strip_control_chars_removes_bare_esc() {
        let raw = "\x1b";
        assert_eq!(strip_control_chars(raw), "");
    }

    #[test]
    fn test_validate_gibo_command_output_target_is_sanitized() {
        let err = validate_gibo_command_output(
            make_status(1),
            String::new(),
            "error",
            "C++\x1b[0m\nFAKE",
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            !msg.chars().any(|c| c.is_control()),
            "control char in error message: {msg:?}"
        );
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
    fn test_validate_gibo_list_output_rejects_empty_output() {
        // exit 0 but no templates: uninitialized or corrupted boilerplate DB
        let err = validate_gibo_list_output(make_status(0), String::new(), "").unwrap_err();
        assert!(err.to_string().contains("empty output"));
    }

    #[test]
    fn test_validate_gibo_list_output_rejects_whitespace_only_output() {
        // exit 0 but only whitespace: should also be treated as empty
        let err =
            validate_gibo_list_output(make_status(0), "   \n\n  \n".to_string(), "").unwrap_err();
        assert!(err.to_string().contains("empty output"));
    }

    #[test]
    fn test_truncate_stderr_short() {
        let s = "short error";
        assert_eq!(truncate_stderr(s), s);
    }

    #[test]
    fn test_truncate_stderr_strips_control_chars() {
        let s = "error: \x1b[31mfailed\x1b[0m";
        let result = truncate_stderr(s);
        assert!(
            !result.contains('\x1b'),
            "ANSI escape sequences should be stripped"
        );
        assert!(
            result.contains("failed"),
            "message content should be preserved"
        );
    }

    #[test]
    fn test_truncate_stderr_long() {
        let s = "x".repeat(MAX_SUBPROCESS_STDERR_BYTES + 100);
        let result = truncate_stderr(&s);
        assert!(result.len() < s.len(), "truncated result should be shorter");
        assert!(
            result.contains("truncated"),
            "should include truncation marker"
        );
    }

    #[test]
    fn test_validate_gibo_command_output_truncates_oversized_stderr() {
        let long_stderr = "e".repeat(MAX_SUBPROCESS_STDERR_BYTES + 1000);
        let err = validate_gibo_command_output(make_status(1), String::new(), &long_stderr, "C++")
            .unwrap_err();
        assert!(err.to_string().contains("truncated"));
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
    fn test_run_command_with_timeout_rejects_oversized_stdout_before_timeout() {
        let args = vec!["-c".to_string(), "trap '' PIPE; yes x; sleep 2".to_string()];
        let started = std::time::Instant::now();
        let err = run_command_with_timeout("sh", &args, Duration::from_secs(2)).unwrap_err();

        assert!(
            err.to_string().contains("subprocess output exceeds"),
            "unexpected error: {err}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "reader error should return before the command timeout"
        );
    }

    #[test]
    fn test_run_command_with_timeout_rejects_shell_grandchild() {
        let args = vec!["-c".to_string(), "sleep 2".to_string()];
        let started = std::time::Instant::now();
        let err = run_command_with_timeout("sh", &args, Duration::from_millis(50)).unwrap_err();

        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "timeout should return before a shell grandchild can keep pipes open"
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

    #[test]
    fn test_read_to_end_rejects_oversized_output() {
        let data = vec![0u8; MAX_SUBPROCESS_OUTPUT_BYTES + 1];
        let reader = std::io::Cursor::new(data);
        let err = read_to_end(reader).unwrap_err();
        assert!(err.to_string().contains("exceeds"));
    }

    #[test]
    fn test_read_to_end_accepts_max_bytes() {
        let data = vec![0u8; MAX_SUBPROCESS_OUTPUT_BYTES];
        let reader = std::io::Cursor::new(data.clone());
        let result = read_to_end(reader).unwrap();
        assert_eq!(result.len(), MAX_SUBPROCESS_OUTPUT_BYTES);
    }
}
