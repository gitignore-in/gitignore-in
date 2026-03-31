use clap::{Parser, Subcommand};
use std::{io::Read, path::Path};
mod build;
mod edit;
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
    /// Search templates available from gibo and gitignore.io
    Search {
        /// Search terms matched case-insensitively against template names
        queries: Vec<String>,
    },
    /// Add templates to .gitignore.in and rebuild .gitignore
    Add {
        /// Template names such as Rust, macOS, or node
        templates: Vec<String>,
    },
    /// Remove templates from .gitignore.in and rebuild .gitignore
    Remove {
        /// Template names such as Rust, macOS, or node
        templates: Vec<String>,
    },
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
        Some(Commands::Search { queries }) => search_templates(queries),
        Some(Commands::Add { templates }) => {
            update_gitignore_in_file(UpdateMode::Add, templates)?;
            println!("Updated .gitignore.in");
            build_gitignore()
        }
        Some(Commands::Remove { templates }) => {
            update_gitignore_in_file(UpdateMode::Remove, templates)?;
            println!("Updated .gitignore.in");
            build_gitignore()
        }
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

enum UpdateMode {
    Add,
    Remove,
}

fn build_gitignore() -> std::io::Result<()> {
    match bootstrap_gitignore_in_file() {
        Ok(BootstrapStatus::Initialized) => {
            println!("Initialized .gitignore.in");
        }
        Ok(BootstrapStatus::Inferred) => {
            println!("Inferred .gitignore.in from .gitignore");
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("Tried to create .gitignore.in, but failed.");
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

fn search_templates(queries: Vec<String>) -> std::io::Result<()> {
    let catalog = edit::Catalog::load()?;
    let results = catalog.search(&queries);
    if results.is_empty() {
        let message = if queries.is_empty() {
            "No templates are available from gibo or gitignore.io".to_string()
        } else {
            format!("No templates matched: {}", queries.join(", "))
        };
        return Err(std::io::Error::other(message));
    }

    for template in results {
        println!(
            "{}\t{}",
            edit::provider_label(template.provider),
            template.target
        );
    }

    Ok(())
}

enum BootstrapStatus {
    AlreadyPresent,
    Initialized,
    Inferred,
}

fn bootstrap_gitignore_in_file() -> std::io::Result<BootstrapStatus> {
    let path = Path::new(".gitignore.in");
    if path.exists() {
        return Ok(BootstrapStatus::AlreadyPresent);
    }

    if Path::new(".gitignore").exists() {
        infer_gitignore_in_file(Vec::new(), Vec::new(), 2)?;
        return Ok(BootstrapStatus::Inferred);
    }

    std::fs::File::create(path)?;

    Ok(BootstrapStatus::Initialized)
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

fn update_gitignore_in_file(mode: UpdateMode, templates: Vec<String>) -> std::io::Result<()> {
    if templates.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "At least one template name is required",
        ));
    }

    match bootstrap_gitignore_in_file() {
        Ok(BootstrapStatus::Initialized) => {
            println!("Initialized .gitignore.in");
        }
        Ok(BootstrapStatus::Inferred) => {
            println!("Inferred .gitignore.in from .gitignore");
        }
        Ok(BootstrapStatus::AlreadyPresent) => {}
        Err(e) => return Err(e),
    }

    let mut script = parse_gitignore_in_file()?;
    match mode {
        UpdateMode::Add => {
            let catalog = edit::Catalog::load()?;
            edit::add_templates(&mut script, &catalog, &templates)?;
        }
        UpdateMode::Remove => {
            edit::remove_templates(&mut script, &templates)?;
        }
    }
    std::fs::write(".gitignore.in", edit::render(&script))?;
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
    use std::sync::{Mutex, OnceLock};

    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_main() {
        let _guard = cwd_lock().lock().expect("failed to lock cwd");
        let current_dir = std::env::current_dir().expect("failed to get current dir");
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
        std::env::set_current_dir(current_dir).expect("failed to restore current dir");
    }

    #[test]
    fn test_bootstrap_infers_from_existing_gitignore() {
        let _guard = cwd_lock().lock().expect("failed to lock cwd");
        let current_dir = std::env::current_dir().expect("failed to get current dir");
        let temp_dir = Temp::new_dir().expect("failed to create temp dir");
        std::env::set_current_dir(temp_dir.as_path()).expect("failed to change current dir");
        std::fs::write(
            ".gitignore",
            "# DO NOT EDIT THIS FILE\n# Generated by gitignore.in\n# Edit .gitignore.in instead of this file\n# Run `gitignore.in` to build .gitignore\n# -----------------------------------------------------------------------------\nplain-entry\n# -----------------------------------------------------------------------------\n!important.txt\n",
        )
        .expect("failed to write .gitignore");

        let result = run(Cli { command: None });
        assert!(result.is_ok());

        let restored =
            std::fs::read_to_string(".gitignore.in").expect("failed to read .gitignore.in");
        assert_eq!(restored, "echo 'plain-entry'\necho '!important.txt'\n");
        std::env::set_current_dir(current_dir).expect("failed to restore current dir");
    }

    #[test]
    fn test_parse_restore_command() {
        let cli = Cli::parse_from(["gitignore.in", "restore"]);
        assert!(matches!(cli.command, Some(Commands::Restore)));
    }

    #[test]
    fn test_parse_add_command() {
        let cli = Cli::parse_from(["gitignore.in", "add", "Rust", "node"]);
        match cli.command {
            Some(Commands::Add { templates }) => {
                assert_eq!(templates, vec!["Rust".to_string(), "node".to_string()]);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_parse_search_command() {
        let cli = Cli::parse_from(["gitignore.in", "search", "rust", "node"]);
        match cli.command {
            Some(Commands::Search { queries }) => {
                assert_eq!(queries, vec!["rust".to_string(), "node".to_string()]);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_parse_remove_command() {
        let cli = Cli::parse_from(["gitignore.in", "remove", "Rust"]);
        match cli.command {
            Some(Commands::Remove { templates }) => {
                assert_eq!(templates, vec!["Rust".to_string()]);
            }
            _ => unreachable!(),
        }
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
