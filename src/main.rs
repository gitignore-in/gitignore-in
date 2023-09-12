use std::{io::Read, path::Path};
mod build;
mod gi;
mod gibo;
mod parser;
mod script;

fn main() -> std::io::Result<()> {
    // check if the .gitignore.in file is in current directory
    // if not, create it
    match ensure_gitignore_in_file() {
        Ok(InitializeStatus::Initialized) => {
            println!("Initialized .gitignore.in");
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("Try to create .gitignore.in file, but failed.");
        }
        Err(e) => {
            println!("Error: {}", e);
            return Err(e);
        }
        _ => {}
    }
    let statements = parse_gitignore_in_file()?;
    let result = build::build(statements)?;
    // write to .gitignore
    ensure_gitignore_file()?;
    let path = Path::new(".gitignore");
    std::fs::write(path, result)?;
    println!("Generated .gitignore");
    Ok(())
}

enum InitializeStatus {
    AlreadyInitialized,
    Initialized,
}

fn ensure_gitignore_in_file() -> std::io::Result<InitializeStatus> {
    let path = Path::new(".gitignore.in");
    if let Err(e) = std::fs::metadata(path) {
        if e.kind() == std::io::ErrorKind::NotFound {
            match std::fs::File::create(path) {
                Ok(_) => return Ok(InitializeStatus::Initialized),
                Err(_) => return Err(e),
            }
        }
    }
    Ok(InitializeStatus::AlreadyInitialized)
}

fn ensure_gitignore_file() -> std::io::Result<()> {
    let path = Path::new(".gitignore");
    if let Err(e) = std::fs::metadata(path) {
        if e.kind() == std::io::ErrorKind::NotFound {
            match std::fs::File::create(path) {
                Ok(_) => return Ok(()),
                Err(_) => return Err(e),
            }
        }
    }
    Ok(())
}

fn parse_gitignore_in_file() -> std::io::Result<script::GitIgnoreIn> {
    let path = std::path::Path::new(".gitignore.in");
    parse_path(path)
}

fn parse_path(path: &Path) -> std::io::Result<script::GitIgnoreIn> {
    let mut file = std::fs::File::open(path)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    let result = parser::parse_text(&content);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mktemp::Temp;

    #[test]
    fn test_main() {
        let temp_dir = Temp::new_dir().expect("failed to create temp dir");
        std::env::set_current_dir(temp_dir.as_path()).expect("failed to change current dir");
        let result = main();
        assert!(result.is_ok());
        // check if the .gitignore.in file is in current directory
        let path = Path::new(".gitignore.in");
        assert!(path.exists());

        // try again
        let result = main();
        assert!(result.is_ok());
        assert!(path.exists());
    }
}
