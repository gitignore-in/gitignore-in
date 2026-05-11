use reqwest::blocking::Client;
use url::Url;

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

pub fn gi_command(target: &str) -> std::io::Result<String> {
    let url = target_url(target)?;
    let client = Client::new();
    let response = match client.get(url.clone()).send() {
        Ok(r) => r,
        Err(e) => {
            return Err(std::io::Error::other(format!(
                "Failed to request to {url}: {e}"
            )));
        }
    };
    let stdout = match response.text() {
        Ok(s) => s,
        Err(e) => {
            return Err(std::io::Error::other(format!(
                "Failed to get {target} from {url}: {e}"
            )));
        }
    };
    if stdout.contains("ERROR") && stdout.contains("is undefined") {
        return Err(std::io::Error::other(format!(
            "Failed to get {target} from {url}: {stdout}"
        )));
    }
    Ok(stdout)
}

pub fn gi_list() -> std::io::Result<Vec<String>> {
    let url = format!("{BASE_URL}list?format=lines");
    let client = Client::new();
    let response = match client.get(url).send() {
        Ok(r) => r,
        Err(e) => {
            return Err(std::io::Error::other(format!(
                "Failed to request to {BASE_URL}list?format=lines: {e}"
            )));
        }
    };
    let stdout = match response.text() {
        Ok(s) => s,
        Err(e) => {
            return Err(std::io::Error::other(format!(
                "Failed to get list from {BASE_URL}list?format=lines: {e}"
            )));
        }
    };
    Ok(stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect())
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
