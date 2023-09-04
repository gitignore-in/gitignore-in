use reqwest::blocking::Client;
pub fn gi_command(target: &str) -> std::io::Result<String> {
    // Request to https://www.toptal.com/developers/gitignore/api/{target}
    let url = format!("https://www.toptal.com/developers/gitignore/api/{}", target);
    let client = Client::new();
    let response = match client.get(url).send() {
        Ok(r) => r,
        Err(e) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "Failed to request to https://www.toptal.com/developers/gitignore/api/{target}: {e}",
                    target = target
                ),
            ));
        }
    };
    let stdout = match response.text() {
        Ok(s) => s,
        Err(e) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to get {target} from https://www.toptal.com/developers/gitignore/api/{target}: {e}", target = target, e = e),
            ));
        }
    };
    Ok(stdout)
}
