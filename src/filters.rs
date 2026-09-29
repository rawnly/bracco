//! Filters applied on top of the fuzzy query (git status, extension, exclude).

use crate::finder::GitState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusFilter {
    #[default]
    All,
    /// Anything git reports (staged, unstaged, untracked, conflicts).
    Changed,
    Staged,
    /// Modified in the worktree, not counting untracked files.
    Unstaged,
    Untracked,
    Clean,
}

impl StatusFilter {
    pub fn matches(self, st: GitState) -> bool {
        match self {
            Self::All => true,
            Self::Changed => !st.is_clean(),
            Self::Staged => st.has_staged(),
            Self::Unstaged => st.y != ' ' && !st.is_untracked(),
            Self::Untracked => st.is_untracked(),
            Self::Clean => st.is_clean(),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Changed => "changed",
            Self::Staged => "staged",
            Self::Unstaged => "unstaged",
            Self::Untracked => "untracked",
            Self::Clean => "clean",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "all" => Self::All,
            "changed" | "modified" => Self::Changed,
            "staged" => Self::Staged,
            "unstaged" => Self::Unstaged,
            "untracked" => Self::Untracked,
            "clean" => Self::Clean,
            _ => return None,
        })
    }

    /// Tab cycle: all → changed → staged → unstaged → untracked → all.
    pub fn next(self) -> Self {
        match self {
            Self::All | Self::Clean => Self::Changed,
            Self::Changed => Self::Staged,
            Self::Staged => Self::Unstaged,
            Self::Unstaged => Self::Untracked,
            Self::Untracked => Self::All,
        }
    }

    /// Switch to `target`, or back to `All` if already there.
    pub fn toggle(self, target: Self) -> Self {
        if self == target { Self::All } else { target }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Filters {
    pub status: StatusFilter,
    /// Allowed extensions, lowercase, no dot. Empty = any. Multiple = OR.
    pub exts: Vec<String>,
    /// Glob patterns to drop. See [`glob_match`].
    pub exclude: Vec<String>,
}

impl Filters {
    pub fn new(status: StatusFilter, exts: &[String], exclude: &[String]) -> Self {
        Self {
            status,
            exts: exts
                .iter()
                .flat_map(|e| e.split(','))
                .map(|e| {
                    e.trim()
                        .trim_start_matches("*.")
                        .trim_start_matches('.')
                        .to_lowercase()
                })
                .filter(|e| !e.is_empty())
                .collect(),
            exclude: exclude.iter().filter(|e| !e.is_empty()).cloned().collect(),
        }
    }

    pub fn has_path_filters(&self) -> bool {
        !self.exts.is_empty() || !self.exclude.is_empty()
    }

    pub fn path_ok(&self, path: &str) -> bool {
        if !self.exts.is_empty() {
            let ext = path
                .rsplit('/')
                .next()
                .and_then(|n| n.rsplit_once('.'))
                .map(|(_, e)| e.to_lowercase());
            if !ext.is_some_and(|e| self.exts.contains(&e)) {
                return false;
            }
        }
        !self.exclude.iter().any(|p| exclude_matches(p, path))
    }
}

/// A pattern without `/` matches any path component (`node_modules`,
/// `*.lock`); with `/` it matches the whole relative path (or a directory
/// prefix of it).
fn exclude_matches(pat: &str, path: &str) -> bool {
    let pat = pat.trim_end_matches('/');
    if pat.contains('/') {
        let pat = pat.trim_start_matches("./");
        glob_match(pat, path)
            || path
                .match_indices('/')
                .any(|(i, _)| glob_match(pat, &path[..i]))
    } else {
        path.split('/').any(|c| glob_match(pat, c))
    }
}

/// Minimal glob: `*` (no `/`), `**` (anything), `?` (one non-`/` char).
pub fn glob_match(pat: &str, text: &str) -> bool {
    fn go(p: &[u8], t: &[u8]) -> bool {
        match p.first() {
            None => t.is_empty(),
            Some(b'*') if p.get(1) == Some(&b'*') => {
                let rest = &p[2..];
                let rest = rest.strip_prefix(b"/").unwrap_or(rest);
                (0..=t.len()).any(|i| go(rest, &t[i..]))
            }
            Some(b'*') => {
                let rest = &p[1..];
                let max = t.iter().position(|&c| c == b'/').unwrap_or(t.len());
                (0..=max).any(|i| go(rest, &t[i..]))
            }
            Some(b'?') => t.first().is_some_and(|&c| c != b'/') && go(&p[1..], &t[1..]),
            Some(&c) => t.first() == Some(&c) && go(&p[1..], &t[1..]),
        }
    }
    go(pat.as_bytes(), text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs() {
        assert!(glob_match("*.lock", "Cargo.lock"));
        assert!(!glob_match("*.lock", "a/Cargo.lock"));
        assert!(glob_match("**/*.lock", "a/b/Cargo.lock"));
        assert!(glob_match("src/**", "src/a/b.rs"));
        assert!(glob_match("a?c", "abc"));
        assert!(!glob_match("a?c", "a/c"));
    }

    #[test]
    fn excludes() {
        let f = Filters::new(
            StatusFilter::All,
            &[],
            &["node_modules".into(), "*.lock".into(), "src/gen".into()],
        );
        assert!(!f.path_ok("a/node_modules/x.js"));
        assert!(!f.path_ok("sub/Cargo.lock"));
        assert!(!f.path_ok("src/gen/x.rs"));
        assert!(f.path_ok("src/main.rs"));
    }

    #[test]
    fn exts() {
        let f = Filters::new(StatusFilter::All, &["rs,.MD".into(), "*.toml".into()], &[]);
        assert!(f.path_ok("a/b.rs") && f.path_ok("README.md") && f.path_ok("Cargo.toml"));
        assert!(!f.path_ok("a.txt") && !f.path_ok("Makefile"));
    }

    #[test]
    fn status() {
        let staged = GitState { x: 'A', y: ' ' };
        let both = GitState { x: 'M', y: 'M' };
        let untracked = GitState { x: '?', y: '?' };
        assert!(StatusFilter::Staged.matches(staged) && StatusFilter::Staged.matches(both));
        assert!(StatusFilter::Unstaged.matches(both) && !StatusFilter::Unstaged.matches(untracked));
        assert!(StatusFilter::Untracked.matches(untracked));
        assert!(StatusFilter::Clean.matches(GitState::CLEAN));
        assert_eq!(
            StatusFilter::Changed.toggle(StatusFilter::Changed),
            StatusFilter::All
        );
    }
}
