pub fn gibo_command(target: &str) -> std::io::Result<String> {
    let output = std::process::Command::new("gibo")
        .arg("dump")
        .arg(target)
        .output()?;
    let stdout = match String::from_utf8(output.stdout) {
        Ok(it) => it,
        Err(err) => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, err)),
    };
    Ok(stdout)
}
