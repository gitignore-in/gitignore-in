use log::debug;
use reqwest::blocking::Client;
use reqwest::StatusCode;
use std::io::ErrorKind;
use std::io::Read;
use std::time::Duration;
use url::Url;

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;
const MAX_RESPONSE_SIZE_DISPLAY: &str = "10 MiB";

fn build_client() -> std::io::Result<Client> {
    Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|e| {
            let (_, reason) = classify_reqwest_error(&e);
            std::io::Error::other(format!("Failed to build HTTP client: {reason}"))
        })
}

const BASE_URL: &str = "https://www.toptal.com/developers/gitignore/api/";
const MAX_ERROR_BODY_CHARS: usize = 200;

fn sanitize_error_body(body: &str) -> String {
    body.chars()
        .filter(|c| !c.is_control())
        .take(MAX_ERROR_BODY_CHARS)
        .collect()
}

pub(crate) fn sanitize_target(target: &str) -> String {
    target.chars().filter(|c| !c.is_control()).collect()
}

fn classify_reqwest_error(e: &reqwest::Error) -> (ErrorKind, &'static str) {
    if e.is_timeout() {
        (ErrorKind::TimedOut, "request timed out")
    } else if e.is_connect() {
        (ErrorKind::NotConnected, "connection failed")
    } else if e.is_builder() {
        (ErrorKind::InvalidInput, "request could not be built")
    } else {
        (ErrorKind::Other, "request failed")
    }
}

fn request_error(url: &str, e: &reqwest::Error) -> std::io::Error {
    let (kind, reason) = classify_reqwest_error(e);
    std::io::Error::new(kind, format!("Failed to request to {url}: {reason}"))
}

fn sanitize_body(body: &str) -> String {
    body.chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
        .collect()
}

fn target_url(target: &str) -> std::io::Result<Url> {
    let mut url = Url::parse(BASE_URL)
        .map_err(|e| std::io::Error::other(format!("Invalid BASE_URL `{BASE_URL}`: {e}")))?;
    url.path_segments_mut()
        .map_err(|()| std::io::Error::other(format!("BASE_URL `{BASE_URL}` is not a base URL")))?
        .pop_if_empty()
        .push(target);
    Ok(url)
}

fn validate_gi_response(
    status: StatusCode,
    body: String,
    target: &str,
    url: &Url,
) -> std::io::Result<String> {
    let target = sanitize_target(target);
    if !status.is_success() {
        let kind = if status.is_server_error() {
            std::io::ErrorKind::ConnectionAborted
        } else {
            std::io::ErrorKind::Other
        };
        return Err(std::io::Error::new(
            kind,
            format!(
                "Failed to get {target} from {url}: HTTP {} (body bytes={})",
                status.as_u16(),
                body.len()
            ),
        ));
    }
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(std::io::Error::other(format!(
            "Failed to get {target} from {url}: response body too large ({} bytes, max {MAX_RESPONSE_SIZE_DISPLAY} / {MAX_RESPONSE_BYTES} bytes)",
            body.len()
        )));
    }
    if body.is_empty() {
        return Err(std::io::Error::other(format!(
            "Failed to get {target} from {url}: empty response body"
        )));
    }
    if body
        .lines()
        .any(|line| line.contains("ERROR") && line.contains(&format!("{target} is undefined")))
    {
        return Err(std::io::Error::other(format!(
            "Failed to get {target} from {url}: {}",
            sanitize_error_body(&body)
        )));
    }
    Ok(sanitize_body(&body))
}

fn parse_gi_list_body(body: &str) -> std::io::Result<Vec<String>> {
    if body.is_empty() {
        return Err(std::io::Error::other("Cached list body is empty"));
    }
    Ok(body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect())
}

fn validate_gi_list_response(
    status: StatusCode,
    body: String,
    url: &str,
) -> std::io::Result<Vec<String>> {
    if !status.is_success() {
        let kind = if status.is_server_error() {
            std::io::ErrorKind::ConnectionAborted
        } else {
            std::io::ErrorKind::Other
        };
        return Err(std::io::Error::new(
            kind,
            format!(
                "Failed to get list from {url}: HTTP {} (body bytes={})",
                status.as_u16(),
                body.len()
            ),
        ));
    }
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(std::io::Error::other(format!(
            "Failed to get list from {url}: response body too large ({} bytes, max {MAX_RESPONSE_SIZE_DISPLAY} / {MAX_RESPONSE_BYTES} bytes)",
            body.len()
        )));
    }
    if body.is_empty() {
        return Err(std::io::Error::other(format!(
            "Failed to get list from {url}: empty response body"
        )));
    }
    Ok(body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect())
}

const USER_AGENT: &str = concat!("gitignore.in/", env!("CARGO_PKG_VERSION"));

pub fn gi_command(target: &str) -> std::io::Result<String> {
    let target = sanitize_target(target);
    let url = target_url(&target)?;
    let url_str = url.as_str();
    let cached = crate::http_cache::get(url_str);
    let client = build_client()?;
    let mut request = client.get(url.clone()).header("User-Agent", USER_AGENT);
    if let Some(ref c) = cached {
        if let Some(ref etag) = c.etag {
            request = request.header("If-None-Match", etag.as_str());
        }
        if let Some(ref lm) = c.last_modified {
            request = request.header("If-Modified-Since", lm.as_str());
        }
    }
    let started = std::time::Instant::now();
    let response = match request.send() {
        Ok(r) => r,
        Err(e) => {
            let (_, reason) = classify_reqwest_error(&e);
            debug!(
                "HTTP GET {url} -> error ({:.0}ms): {reason}",
                started.elapsed().as_millis()
            );
            return Err(request_error(url.as_str(), &e));
        }
    };
    let status = response.status();
    debug!(
        "HTTP GET {url} -> {status} ({:.0}ms)",
        started.elapsed().as_millis()
    );
    if status == StatusCode::NOT_MODIFIED {
        if let Some(entry) = cached {
            return Ok(entry.body);
        }
        return Err(std::io::Error::other(format!(
            "Got 304 Not Modified from {url} but no cached body"
        )));
    }
    let etag = response
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let last_modified = response
        .headers()
        .get("last-modified")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let body = read_response_body_string(response, &format!("get {target} from {url}"))?;
    let result = validate_gi_response(status, body, &target, &url)?;
    crate::http_cache::put(
        url_str,
        &crate::http_cache::CacheEntry {
            etag,
            last_modified,
            body: result.clone(),
        },
    );
    Ok(result)
}

pub fn gi_list() -> std::io::Result<Vec<String>> {
    let url = format!("{BASE_URL}list?format=lines");
    let cached = crate::http_cache::get(&url);
    let client = build_client()?;
    let mut request = client.get(&url).header("User-Agent", USER_AGENT);
    if let Some(ref c) = cached {
        if let Some(ref etag) = c.etag {
            request = request.header("If-None-Match", etag.as_str());
        }
        if let Some(ref lm) = c.last_modified {
            request = request.header("If-Modified-Since", lm.as_str());
        }
    }
    let started = std::time::Instant::now();
    let response = match request.send() {
        Ok(r) => r,
        Err(e) => {
            let (_, reason) = classify_reqwest_error(&e);
            debug!(
                "HTTP GET {url} -> error ({:.0}ms): {reason}",
                started.elapsed().as_millis()
            );
            return Err(request_error(&url, &e));
        }
    };
    let status = response.status();
    debug!(
        "HTTP GET {url} -> {status} ({:.0}ms)",
        started.elapsed().as_millis()
    );
    if status == StatusCode::NOT_MODIFIED {
        if let Some(entry) = cached {
            return parse_gi_list_body(&entry.body);
        }
        return Err(std::io::Error::other(format!(
            "Got 304 Not Modified from {url} but no cached body"
        )));
    }
    let etag = response
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let last_modified = response
        .headers()
        .get("last-modified")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let body = read_response_body_string(response, &format!("get list from {url}"))?;
    let result = validate_gi_list_response(status, body.clone(), &url)?;
    crate::http_cache::put(
        &url,
        &crate::http_cache::CacheEntry {
            etag,
            last_modified,
            body,
        },
    );
    Ok(result)
}

fn read_response_body_string(
    response: reqwest::blocking::Response,
    context: &str,
) -> std::io::Result<String> {
    let limit = MAX_RESPONSE_BYTES as u64 + 1;
    let mut buf = Vec::new();
    if let Err(e) = response.take(limit).read_to_end(&mut buf) {
        return Err(std::io::Error::other(format!("Failed to {context}: {e}")));
    }
    if buf.len() as u64 >= limit {
        return Err(std::io::Error::other(format!(
            "Failed to {context}: response body too large (> {MAX_RESPONSE_SIZE_DISPLAY} / {MAX_RESPONSE_BYTES} bytes)"
        )));
    }
    String::from_utf8(buf).map_err(|e| std::io::Error::other(format!("Failed to {context}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_url_plain() {
        assert_eq!(
            target_url("Rust").unwrap().as_str(),
            "https://www.toptal.com/developers/gitignore/api/Rust"
        );
    }

    #[test]
    fn test_target_url_encodes_hash() {
        // `C#` / `F#` のように `#` を含む実在の言語名でも、fragment として
        // 解釈されずに path segment に届くこと。
        assert_eq!(
            target_url("C#").unwrap().as_str(),
            "https://www.toptal.com/developers/gitignore/api/C%23"
        );
    }

    #[test]
    fn test_target_url_encodes_question_mark() {
        // `?` を含む target で query string 区切りに解釈されないこと。
        assert_eq!(
            target_url("templ?foo").unwrap().as_str(),
            "https://www.toptal.com/developers/gitignore/api/templ%3Ffoo"
        );
    }

    #[test]
    fn test_target_url_encodes_slash() {
        // path segment 内の `/` は escape されること (深い path を作らない)。
        assert_eq!(
            target_url("foo/bar").unwrap().as_str(),
            "https://www.toptal.com/developers/gitignore/api/foo%2Fbar"
        );
    }

    #[test]
    fn test_target_url_encodes_space() {
        assert_eq!(
            target_url("hello world").unwrap().as_str(),
            "https://www.toptal.com/developers/gitignore/api/hello%20world"
        );
    }

    #[test]
    fn test_target_url_preserves_plus() {
        // `C++` のように既存テストで使われている文字は従来通り通る。
        assert_eq!(
            target_url("C++").unwrap().as_str(),
            "https://www.toptal.com/developers/gitignore/api/C++"
        );
    }

    fn dummy_url() -> Url {
        Url::parse("https://example.test/api/X").unwrap()
    }

    #[test]
    fn test_validate_gi_response_ok() {
        let body = "### X ###\nfoo\n".to_string();
        let result = validate_gi_response(StatusCode::OK, body.clone(), "X", &dummy_url()).unwrap();
        assert_eq!(result, body);
    }

    #[test]
    fn test_validate_gi_response_rejects_5xx_with_empty_body() {
        let err = validate_gi_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            String::new(),
            "X",
            &dummy_url(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("HTTP 500"));
        assert_eq!(err.kind(), std::io::ErrorKind::ConnectionAborted);
    }

    #[test]
    fn test_validate_gi_response_rejects_5xx_with_html_body() {
        let html = "<html><body>503 Service Unavailable</body></html>".to_string();
        let err = validate_gi_response(StatusCode::SERVICE_UNAVAILABLE, html, "X", &dummy_url())
            .unwrap_err();
        assert!(err.to_string().contains("HTTP 503"));
        assert_eq!(err.kind(), std::io::ErrorKind::ConnectionAborted);
    }

    #[test]
    fn test_validate_gi_response_rejects_4xx_with_other_kind() {
        let err = validate_gi_response(
            StatusCode::NOT_FOUND,
            "Not found".to_string(),
            "X",
            &dummy_url(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("HTTP 404"));
        assert_eq!(err.kind(), std::io::ErrorKind::Other);
    }

    #[test]
    fn test_validate_gi_response_rejects_2xx_with_empty_body() {
        // upstream が 200 を返したのに body が空なら、`.gitignore` に空文字を
        // 書き込まないように reject する。
        let err =
            validate_gi_response(StatusCode::OK, String::new(), "X", &dummy_url()).unwrap_err();
        assert!(err.to_string().contains("empty response body"));
    }

    #[test]
    fn test_validate_gi_response_rejects_legacy_error_marker() {
        // gitignore.io 既存のエラー文面 "#!! ERROR: <target> is undefined." を
        // 200 で返してくる経路の互換性も維持する。
        let body = "#!! ERROR: foo is undefined. #!!".to_string();
        let err = validate_gi_response(StatusCode::OK, body, "foo", &dummy_url()).unwrap_err();
        assert!(err.to_string().contains("ERROR"));
        assert!(err.to_string().contains("is undefined"));
    }

    #[test]
    fn test_validate_gi_response_does_not_reject_independent_error_keywords() {
        // "ERROR" and "is undefined" appearing in unrelated parts of a valid template
        // body must not trigger the legacy error detector.
        let body = "# ERROR handling notes\n# Check if value is undefined before use".to_string();
        let result = validate_gi_response(StatusCode::OK, body, "rust", &dummy_url());
        assert!(
            result.is_ok(),
            "unrelated occurrences of ERROR and 'is undefined' must not cause false positive"
        );
    }

    #[test]
    fn test_sanitize_target_strips_control_chars() {
        assert_eq!(sanitize_target("Rust\x1b[0m"), "Rust[0m");
        assert_eq!(sanitize_target("Rust\nFAKE"), "RustFAKE");
        assert_eq!(sanitize_target("Rust\x00null"), "Rustnull");
    }

    #[test]
    fn test_sanitize_target_preserves_normal_names() {
        assert_eq!(sanitize_target("Rust"), "Rust");
        assert_eq!(sanitize_target("C++"), "C++");
        assert_eq!(sanitize_target("C#"), "C#");
    }

    #[test]
    fn test_validate_gi_response_target_is_sanitized_in_error() {
        let err = validate_gi_response(
            StatusCode::NOT_FOUND,
            "not found".to_string(),
            "Rust\x1b[0m\nFAKE",
            &dummy_url(),
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            !msg.chars().any(|c| c.is_control()),
            "control char in error message: {msg:?}"
        );
    }

    #[test]
    fn test_sanitize_body_strips_non_whitespace_control_chars() {
        // ESC (0x1b) and NUL (0x00) are removed; \n and \t are preserved.
        let body = "\x00### template ###\n\x1b[32mfoo\x1b[0m\n\tindented\n";
        assert_eq!(
            sanitize_body(body),
            "### template ###\n[32mfoo[0m\n\tindented\n"
        );
    }

    #[test]
    fn test_validate_gi_response_success_body_has_control_chars_stripped() {
        // A 200 response body containing ANSI escapes must have those stripped
        // before the content is returned to callers.
        let body = "### X ###\n\x1b[32mfoo\x1b[0m\nbar\n".to_string();
        let result = validate_gi_response(StatusCode::OK, body, "X", &dummy_url()).unwrap();
        assert!(!result
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')));
        assert!(
            result.contains("foo"),
            "printable content must be preserved"
        );
        assert!(result.contains("bar"));
    }

    fn test_sanitize_error_body_strips_control_chars() {
        // ESC (0x1b) and newline (0x0a) are control characters and are removed.
        // Printable ANSI parameter bytes like '[', '3', '1', 'm' are kept but
        // are harmless without the preceding ESC byte.
        let body = "\x1b[31mERROR\x1b[0m: foo is undefined.\x0a";
        assert_eq!(sanitize_error_body(body), "[31mERROR[0m: foo is undefined.");
    }

    #[test]
    fn test_sanitize_error_body_truncates_long_input() {
        let long = "A".repeat(MAX_ERROR_BODY_CHARS + 50);
        let result = sanitize_error_body(&long);
        assert_eq!(result.chars().count(), MAX_ERROR_BODY_CHARS);
    }

    #[test]
    fn test_request_error_omits_reqwest_error_display() {
        let err = Client::new()
            .get("http://user:password@")
            .send()
            .expect_err("invalid URL should fail before making a request");
        let safe = request_error("https://example.test/api/Rust", &err);
        let msg = safe.to_string();

        assert!(msg.contains("https://example.test/api/Rust"));
        assert!(!msg.contains("user:password"));
        assert!(!msg.contains("http://user"));
        assert!(!msg.contains(&err.to_string()));
    }

    #[test]
    fn test_validate_gi_response_error_body_is_sanitized() {
        // Embedded control characters in the API response body must not reach
        // the error message verbatim.
        let body = format!(
            "\x1b[31mERROR\x1b[0m: rust is undefined.\x0a{}",
            "x".repeat(300)
        );
        let err = validate_gi_response(StatusCode::OK, body, "rust", &dummy_url()).unwrap_err();
        let msg = err.to_string();
        // Control characters must be absent.
        assert!(
            !msg.chars().any(|c| c.is_control()),
            "control char in: {msg:?}"
        );
        // Message is bounded (url/target prefix + up to MAX_ERROR_BODY_CHARS body).
        assert!(
            msg.len() < 512,
            "error message unexpectedly long: {} chars",
            msg.len()
        );
    }

    #[test]
    fn test_validate_gi_list_response_ok() {
        let body = "Rust\nGo\nPython\n".to_string();
        let result =
            validate_gi_list_response(StatusCode::OK, body, "https://example.test/list").unwrap();
        assert_eq!(result, vec!["Rust", "Go", "Python"]);
    }

    #[test]
    fn test_validate_gi_list_response_rejects_non_2xx() {
        let err = validate_gi_list_response(
            StatusCode::BAD_GATEWAY,
            "<html>502</html>".to_string(),
            "https://example.test/list",
        )
        .unwrap_err();
        assert!(err.to_string().contains("HTTP 502"));
        assert_eq!(err.kind(), std::io::ErrorKind::ConnectionAborted);
    }

    #[test]
    fn test_validate_gi_list_response_rejects_4xx_with_other_kind() {
        let err = validate_gi_list_response(
            StatusCode::NOT_FOUND,
            "Not found".to_string(),
            "https://example.test/list",
        )
        .unwrap_err();
        assert!(err.to_string().contains("HTTP 404"));
        assert_eq!(err.kind(), std::io::ErrorKind::Other);
    }

    #[test]
    fn test_validate_gi_list_response_rejects_2xx_empty_body() {
        let err =
            validate_gi_list_response(StatusCode::OK, String::new(), "https://example.test/list")
                .unwrap_err();
        assert!(err.to_string().contains("empty response body"));
    }

    #[test]
    fn test_validate_gi_response_rejects_oversized_body() {
        let body = "x".repeat(MAX_RESPONSE_BYTES + 1);
        let err = validate_gi_response(StatusCode::OK, body, "X", &dummy_url()).unwrap_err();
        assert!(err.to_string().contains("too large"));
    }

    #[test]
    fn test_validate_gi_list_response_rejects_oversized_body() {
        let body = "x".repeat(MAX_RESPONSE_BYTES + 1);
        let err = validate_gi_list_response(StatusCode::OK, body, "https://example.test/list")
            .unwrap_err();
        assert!(err.to_string().contains("too large"));
    }

    #[test]
    fn test_gi_command() {
        let result = gi_command("C++");
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.contains("### C++ ###"));
    }

    #[test]
    fn test_gi_command_fail() {
        let result = gi_command("unknown-language");
        assert!(result.is_err());
    }
}
