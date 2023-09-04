// build .gitignore from .gitignore.in script

use crate::{
    gi::gi_command,
    gibo::gibo_command,
    script::{Comment, Echo, Gi, Gibo, GitIgnoreIn, GitIgnoreStatement, Meaningless},
};

pub(crate) fn build(script: GitIgnoreIn) -> std::io::Result<String> {
    let mut result = String::new();
    result.push_str("# DO NOT EDIT THIS FILE\n");
    result.push_str("# Generated by gitignore.in\n");
    result.push_str("# Edit .gitignore.in instead of this file\n");
    result.push_str("# Run `gitignore.in` to build .gitignore\n");
    for statement in script.content {
        match statement {
            GitIgnoreStatement::Comment(Comment::Content(c)) => {
                result.push_str(&format!("# {}\n", c));
            }
            GitIgnoreStatement::Meaningless(Meaningless::Content(_m)) => {}
            GitIgnoreStatement::Gibo(Gibo::Target(target)) => {
                let content = gibo_command(&target)?;
                result.push_str(&separator());
                result.push_str(&format!("# gibo dump {}\n", target));
                result.push_str(&content);
            }
            GitIgnoreStatement::Gi(Gi::Target(target)) => {
                let content = gi_command(&target)?;
                result.push_str(&separator());
                result.push_str(&format!("# gi {}\n", target));
                result.push_str(&content);
            }
            GitIgnoreStatement::Echo(Echo::Content(echo)) => {
                result.push_str(&separator());
                result.push_str(&format!("{}\n", echo));
            }
        }
    }
    Ok(result)
}

fn separator() -> String {
    "# -----------------------------------------------------------------------------\n".to_string()
}
