use log::debug;
use reqwest::blocking::Client;
use reqwest::StatusCode;
use std::io::Read;
use std::time::Duration;
use url::Url;

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;

fn build_client() -> std::io::Result<Client> {
    Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|e| std::io::Error::other(format!("Failed to build HTTP client: {e}")))
}

const BASE_URL: &str = "https://www.toptal.com/developers/gitignore/api/";

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
            "Failed to get {target} from {url}: response body too large ({} bytes, max {MAX_RESPONSE_BYTES})",
            body.len()
        )));
    }
    if body.is_empty() {
        return Err(std::io::Error::other(format!(
            "Failed to get {target} from {url}: empty response body"
        )));
    }
    if body.contains("ERROR") && body.contains("is undefined") {
        return Err(std::io::Error::other(format!(
            "Failed to get {target} from {url}: {body}"
        )));
    }
    Ok(body)
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
            "Failed to get list from {url}: response body too large ({} bytes, max {MAX_RESPONSE_BYTES})",
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
    let url = target_url(target)?;
    let client = build_client()?;
    let started = std::time::Instant::now();
    let response = match client
        .get(url.clone())
        .header("User-Agent", USER_AGENT)
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            let kind = if e.is_timeout() {
                std::io::ErrorKind::TimedOut
            } else if e.is_connect() {
                std::io::ErrorKind::NotConnected
            } else {
                std::io::ErrorKind::Other
            };
            debug!(
                "HTTP GET {url} -> error ({:.0}ms): {e}",
                started.elapsed().as_millis()
            );
            return Err(std::io::Error::new(
                kind,
                format!("Failed to request to {url}: {e}"),
            ));
        }
    };
    let status = response.status();
    debug!(
        "HTTP GET {url} -> {status} ({:.0}ms)",
        started.elapsed().as_millis()
    );
    let body = read_response_body_string(response, &format!("get {target} from {url}"))?;
    validate_gi_response(status, body, target, &url)
}

pub fn gi_list() -> std::io::Result<Vec<String>> {
    let url = format!("{BASE_URL}list?format=lines");
    let client = build_client()?;
    let started = std::time::Instant::now();
    let response = match client.get(&url).header("User-Agent", USER_AGENT).send() {
        Ok(r) => r,
        Err(e) => {
            let kind = if e.is_timeout() {
                std::io::ErrorKind::TimedOut
            } else if e.is_connect() {
                std::io::ErrorKind::NotConnected
            } else {
                std::io::ErrorKind::Other
            };
            debug!(
                "HTTP GET {url} -> error ({:.0}ms): {e}",
                started.elapsed().as_millis()
            );
            return Err(std::io::Error::new(
                kind,
                format!("Failed to request to {url}: {e}"),
            ));
        }
    };
    let status = response.status();
    debug!(
        "HTTP GET {url} -> {status} ({:.0}ms)",
        started.elapsed().as_millis()
    );
    let body = read_response_body_string(response, &format!("get list from {url}"))?;
    validate_gi_list_response(status, body, &url)
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
            "Failed to {context}: response body too large (> {MAX_RESPONSE_BYTES} bytes)"
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
