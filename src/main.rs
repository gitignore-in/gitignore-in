use clap::{Parser, Subcommand};
use std::{io::Read, path::Path};
mod build;
mod gi;
mod gibo;
mod infer;
mod parser;
mod restore;
mod script;

fn main() -> std::io::Result<()> {
    let cli = Cli::parse();
    run(cli)
}

#[derive(Debug, Parser)]
#[command(
    name = "gitignore.in",
    version,
    about = "Manage .gitignore files with .gitignore.in",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Restore .gitignore.in from a generated .gitignore
    Restore,
    /// Infer .gitignore.in from an existing .gitignore
    Infer {
        /// Comma-separated gibo targets to consider
        #[arg(long, value_delimiter = ',')]
        gibo: Vec<String>,
        /// Comma-separated gitignore.io targets to consider
        #[arg(long, value_delimiter = ',')]
        gi: Vec<String>,
        /// Minimum number of matching lines required for a template
        #[arg(long, default_value_t = 2)]
        min_overlap: usize,
    },
}

fn run(cli: Cli) -> std::io::Result<()> {
    match cli.command {
        Some(Commands::Restore) => {
            restore_gitignore_in_file()?;
            println!("Restored .gitignore.in");
            Ok(())
        }
        Some(Commands::Infer {
            gibo,
            gi,
            min_overlap,
        }) => {
            infer_gitignore_in_file(gibo, gi, min_overlap)?;
            println!("Inferred .gitignore.in");
            Ok(())
        }
        None => build_gitignore(),
    }
}

fn build_gitignore() -> std::io::Result<()> {
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
            println!("Error: {e}");
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

fn restore_gitignore_in_file() -> std::io::Result<()> {
    let path = std::path::Path::new(".gitignore");
    let mut file = std::fs::File::open(path)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    let restored = restore::restore(&content);
    std::fs::write(".gitignore.in", restored)?;
    Ok(())
}

fn infer_gitignore_in_file(
    gibo_targets: Vec<String>,
    gi_targets: Vec<String>,
    min_overlap: usize,
) -> std::io::Result<()> {
    let path = std::path::Path::new(".gitignore");
    let mut file = std::fs::File::open(path)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    let inferred = infer::infer_with_options(
        &content,
        &infer::InferOptions {
            gibo_targets,
            gi_targets,
            min_overlap,
        },
    )?;
    std::fs::write(".gitignore.in", inferred)?;
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
        let result = run(Cli { command: None });
        assert!(result.is_ok());
        // check if the .gitignore.in file is in current directory
        let path = Path::new(".gitignore.in");
        assert!(path.exists());

        // try again
        let result = run(Cli { command: None });
        assert!(result.is_ok());
        assert!(path.exists());
    }

    #[test]
    fn test_parse_restore_command() {
        let cli = Cli::parse_from(["gitignore.in", "restore"]);
        assert!(matches!(cli.command, Some(Commands::Restore)));
    }

    #[test]
    fn test_parse_infer_command() {
        let cli = Cli::parse_from([
            "gitignore.in",
            "infer",
            "--gibo",
            "Rust,macOS",
            "--gi",
            "node",
            "--min-overlap",
            "3",
        ]);

        match cli.command {
            Some(Commands::Infer {
                gibo,
                gi,
                min_overlap,
            }) => {
                assert_eq!(gibo, vec!["Rust".to_string(), "macOS".to_string()]);
                assert_eq!(gi, vec!["node".to_string()]);
                assert_eq!(min_overlap, 3);
            }
            _ => unreachable!(),
        }
    }
}
