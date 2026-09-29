use std::path::Path;

use eyre::{Result, eyre};
use fff_search::{
    FFFMode, FilePicker, FilePickerOptions, FuzzyQuery, FuzzySearchOptions, PaginationArgs,
    QueryParser,
};

use crate::filters::Filters;
use tracing::{debug, info, instrument};

/// Porcelain-style two column status, like `git status --short` / lazygit:
/// `x` = index (staged) column, `y` = worktree (unstaged) column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitState {
    pub x: char,
    pub y: char,
}

impl GitState {
    pub const CLEAN: Self = Self { x: ' ', y: ' ' };

    pub fn is_clean(self) -> bool {
        self == Self::CLEAN
    }
    pub fn is_conflicted(self) -> bool {
        self.x == 'U'
            || self.y == 'U'
            || (self.x == 'A' && self.y == 'A')
            || (self.x == 'D' && self.y == 'D')
    }
    pub fn is_untracked(self) -> bool {
        self.x == '?'
    }
    pub fn has_staged(self) -> bool {
        !matches!(self.x, ' ' | '?')
    }
    pub fn has_unstaged(self) -> bool {
        self.y != ' ' || self.is_untracked()
    }
}

fn state(f: &fff_search::FileItem) -> GitState {
    let Some(s) = f.git_status else {
        return GitState::CLEAN;
    };
    if s.is_conflicted() {
        return GitState { x: 'U', y: 'U' };
    }
    if s.is_wt_new() && !s.is_index_new() {
        return GitState { x: '?', y: '?' };
    }
    let x = if s.is_index_new() {
        'A'
    } else if s.is_index_modified() {
        'M'
    } else if s.is_index_deleted() {
        'D'
    } else if s.is_index_renamed() {
        'R'
    } else if s.is_index_typechange() {
        'T'
    } else {
        ' '
    };
    let y = if s.is_wt_modified() {
        'M'
    } else if s.is_wt_deleted() {
        'D'
    } else if s.is_wt_renamed() {
        'R'
    } else if s.is_wt_typechange() {
        'T'
    } else {
        ' '
    };
    GitState { x, y }
}

#[derive(Debug, Clone)]
pub struct Entry {
    /// Path relative to the finder root.
    pub path: String,
    pub state: GitState,
}

pub struct SearchResult {
    pub entries: Vec<Entry>,
    /// Total number of matches before truncation to `limit`.
    pub total: usize,
}

pub struct Finder {
    picker: FilePicker,
}

impl Finder {
    /// Index `root` (respects .gitignore). Synchronous: git statuses are
    /// applied before this returns.
    #[instrument(skip_all, fields(root = %root.display()))]
    pub fn open(root: &Path) -> Result<Self> {
        let mut picker = FilePicker::new(FilePickerOptions {
            base_path: root.to_string_lossy().into_owned(),
            mode: FFFMode::Neovim,
            watch: false,
            ..Default::default()
        })
        .map_err(|e| eyre!("cannot index {}: {e}", root.display()))?;

        let t = std::time::Instant::now();
        picker
            .collect_files()
            .map_err(|e| eyre!("cannot scan {}: {e}", root.display()))?;
        info!(files = picker.get_files().len(), elapsed = ?t.elapsed(), "scan finished");

        Ok(Self { picker })
    }

    /// Re-scan the tree and re-read git status (files may have changed while
    /// an external command had the terminal).
    #[instrument(skip_all)]
    pub fn refresh(&mut self) -> Result<()> {
        let t = std::time::Instant::now();
        self.picker
            .collect_files()
            .map_err(|e| eyre!("cannot rescan: {e}"))?;
        info!(files = self.picker.get_files().len(), elapsed = ?t.elapsed(), "rescan finished");
        Ok(())
    }

    /// Files matching `query` and `filters` (all files if empty), git-changed
    /// ones first. The partition happens over *all* matches, then the result
    /// is truncated to `limit`.
    #[instrument(skip(self))]
    pub fn search(&self, query: &str, filters: &Filters, limit: usize) -> SearchResult {
        let picker = &self.picker;
        let parsed = QueryParser::default().parse(query);

        // fff only fuzzy-matches text of 2+ bytes. For 0/1 char text we list
        // the (constraint-filtered) files ourselves, sorted by path.
        let short_needle: Option<String> = match &parsed.fuzzy_query {
            FuzzyQuery::Empty => Some(String::new()),
            FuzzyQuery::Text(t) if t.len() < 2 => Some(t.to_lowercase()),
            _ => None,
        };

        let mut candidates: Vec<&fff_search::FileItem> =
            if short_needle.is_some() && parsed.constraints.is_empty() {
                // get_files() is already sorted by path.
                picker
                    .get_files()
                    .iter()
                    .filter(|f| !f.is_deleted())
                    .collect()
            } else {
                let res = picker.fuzzy_search(
                    &parsed,
                    None,
                    FuzzySearchOptions {
                        max_threads: 0,
                        // limit 0 == all matches
                        pagination: PaginationArgs {
                            offset: 0,
                            limit: 0,
                        },
                        ..Default::default()
                    },
                );
                let mut items = res.items;
                if short_needle.is_some() {
                    items.sort_by_cached_key(|f| f.relative_path(picker));
                }
                items
            };

        if let Some(needle) = short_needle.as_deref().filter(|n| !n.is_empty()) {
            candidates.retain(|f| f.relative_path(picker).to_lowercase().contains(needle));
        }

        let mut changed = Vec::new();
        let mut clean = Vec::new();
        for f in candidates {
            let st = state(f);
            if !filters.status.matches(st) {
                continue;
            }
            if filters.has_path_filters() && !filters.path_ok(&f.relative_path(picker)) {
                continue;
            }
            if st.is_clean() {
                clean.push((f, st))
            } else {
                changed.push((f, st))
            }
        }

        // Stable partition: relevance / alphabetical order kept within groups.
        let total = changed.len() + clean.len();
        let entries: Vec<Entry> = changed
            .into_iter()
            .chain(clean)
            .take(limit)
            .map(|(f, state)| Entry {
                path: f.relative_path(picker),
                state,
            })
            .collect();
        debug!(results = entries.len(), total, "search done");
        SearchResult { entries, total }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .args(["-c", "user.email=a@b", "-c", "user.name=t"])
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    fn repo() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let p = d.path();
        let _ = p;
        git(p, &["init", "-q"]);
        std::fs::write(p.join(".gitignore"), "ignored.log\n").unwrap();
        for f in ["a.rs", "b.md", "c.toml"] {
            std::fs::write(p.join(f), "1\n").unwrap();
        }
        std::fs::write(p.join("ignored.log"), "x").unwrap();
        git(p, &["add", "-A"]);
        git(p, &["commit", "-qm", "i"]);
        std::fs::write(p.join("c.toml"), "2\n").unwrap(); // unstaged
        std::fs::write(p.join("new.txt"), "n").unwrap(); // untracked
        d
    }

    fn paths(r: &SearchResult) -> Vec<&str> {
        r.entries.iter().map(|e| e.path.as_str()).collect()
    }

    #[test]
    fn changed_first_and_gitignore_respected() {
        let d = repo();
        let f = Finder::open(&d.path().canonicalize().unwrap()).unwrap();
        let r = f.search("", &Filters::default(), 100);
        let p = paths(&r);
        assert!(!p.contains(&"ignored.log"));
        assert_eq!(&p[..2], ["c.toml", "new.txt"]);
        assert_eq!(r.total, p.len());
    }

    #[test]
    fn one_char_query_filters() {
        let d = repo();
        let f = Finder::open(&d.path().canonicalize().unwrap()).unwrap();
        assert_eq!(paths(&f.search("w", &Filters::default(), 100)), ["new.txt"]);
    }

    #[test]
    fn limit_applies_after_partition() {
        let d = repo();
        let f = Finder::open(&d.path().canonicalize().unwrap()).unwrap();
        let r = f.search("", &Filters::default(), 1);
        assert_eq!(r.entries.len(), 1);
        assert!(!r.entries[0].state.is_clean());
        assert!(r.total > 1);
    }

    #[test]
    fn status_and_constraint_filters() {
        use crate::filters::StatusFilter;
        let d = repo();
        let f = Finder::open(&d.path().canonicalize().unwrap()).unwrap();
        let only = |st| Filters::new(st, &[], &[]);
        assert_eq!(
            paths(&f.search("", &only(StatusFilter::Untracked), 100)),
            ["new.txt"]
        );
        assert_eq!(
            paths(&f.search("", &only(StatusFilter::Unstaged), 100)),
            ["c.toml"]
        );
        let ext = Filters::new(StatusFilter::All, &["md".into()], &[]);
        assert_eq!(paths(&f.search("", &ext, 100)), ["b.md"]);
        let ex = Filters::new(StatusFilter::All, &[], &["*.rs".into()]);
        assert!(!paths(&f.search("", &ex, 100)).contains(&"a.rs"));
        // fff query syntax works even with no fuzzy text
        let all = Filters::default();
        assert_eq!(paths(&f.search("status:untracked", &all, 100)), ["new.txt"]);
        assert_eq!(paths(&f.search("*.rs", &all, 100)), ["a.rs"]);
        // constraint + 1-char text
        assert_eq!(paths(&f.search("*.toml c", &all, 100)), ["c.toml"]);
    }

    #[test]
    fn refresh_picks_up_changes() {
        let d = repo();
        let root = d.path().canonicalize().unwrap();
        let mut f = Finder::open(&root).unwrap();
        assert!(
            f.search("fresh", &Filters::default(), 10)
                .entries
                .is_empty()
        );
        std::fs::write(root.join("fresh.rs"), "x").unwrap();
        std::fs::write(root.join("a.rs"), "changed").unwrap(); // was clean
        f.refresh().unwrap();
        let r = f.search("", &Filters::default(), 100);
        let p = paths(&r);
        assert!(p.contains(&"fresh.rs"));
        assert!(
            p[..3].contains(&"a.rs"),
            "newly modified file sorts first: {p:?}"
        );
    }

    #[test]
    fn states() {
        let d = repo();
        let f = Finder::open(&d.path().canonicalize().unwrap()).unwrap();
        let r = f.search("", &Filters::default(), 100);
        let get = |n: &str| r.entries.iter().find(|e| e.path == n).unwrap().state;
        assert_eq!(get("c.toml"), GitState { x: ' ', y: 'M' });
        assert_eq!(get("new.txt"), GitState { x: '?', y: '?' });
        assert!(get("a.rs").is_clean());
    }
}
