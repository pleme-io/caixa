//! URL shorthand expansion.
//!
//! Accepts the same shorthand Nix flakes do, plus friendly aliases:
//!   - `github:<org>/<repo>`  → `https://github.com/<org>/<repo>.git`
//!   - `gitlab:<org>/<repo>`  → `https://gitlab.com/<org>/<repo>.git`
//!   - `codeberg:<org>/<repo>` → `https://codeberg.org/<org>/<repo>.git`
//!   - already-explicit URLs (`https://…`, `ssh://…`, `git@…:…`) pass through.

/// Expand a shorthand to a concrete `git clone`-ready URL.
#[must_use]
pub fn expand_shorthand(repo: &str) -> String {
    if repo.contains("://")
        || repo.starts_with("git@")
        || repo.starts_with('/')
        || repo.starts_with('.')
    {
        return repo.to_string();
    }
    if let Some((prefix, path)) = repo.split_once(':') {
        match prefix {
            "github" => return format!("https://github.com/{path}.git"),
            "gitlab" => return format!("https://gitlab.com/{path}.git"),
            "codeberg" => return format!("https://codeberg.org/{path}.git"),
            "sourcehut" | "sr.ht" => return format!("https://git.sr.ht/~{path}"),
            _ => {}
        }
    }
    repo.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_expands() {
        assert_eq!(
            expand_shorthand("github:pleme-io/caixa"),
            "https://github.com/pleme-io/caixa.git"
        );
    }

    #[test]
    fn gitlab_expands() {
        assert_eq!(
            expand_shorthand("gitlab:my-org/proj"),
            "https://gitlab.com/my-org/proj.git"
        );
    }

    #[test]
    fn https_passes_through() {
        let url = "https://github.com/pleme-io/caixa.git";
        assert_eq!(expand_shorthand(url), url);
    }

    #[test]
    fn ssh_passes_through() {
        let url = "git@github.com:pleme-io/caixa.git";
        assert_eq!(expand_shorthand(url), url);
    }

    #[test]
    fn local_path_passes_through() {
        assert_eq!(expand_shorthand("../caixa-teia"), "../caixa-teia");
        assert_eq!(expand_shorthand("/abs/path"), "/abs/path");
    }
}
