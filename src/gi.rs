use reqwest::blocking::Client;

const BASE_URL: &str = "https://www.toptal.com/developers/gitignore/api/";

pub fn gi_command(target: &str) -> std::io::Result<String> {
    // Request to https://www.toptal.com/developers/gitignore/api/{target}
    let url = format!("{BASE_URL}{target}");
    let client = Client::new();
    let response = match client.get(url).send() {
        Ok(r) => r,
        Err(e) => {
            return Err(std::io::Error::other(format!(
                "Failed to request to {BASE_URL}{target}: {e}"
            )));
        }
    };
    let stdout = match response.text() {
        Ok(s) => s,
        Err(e) => {
            return Err(std::io::Error::other(format!(
                "Failed to get {target} from {BASE_URL}{target}: {e}"
            )));
        }
    };
    if stdout.contains("ERROR") && stdout.contains("is undefined") {
        return Err(std::io::Error::other(format!(
            "Failed to get {target} from {BASE_URL}{target}: {stdout}"
        )));
    }
    Ok(stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

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
