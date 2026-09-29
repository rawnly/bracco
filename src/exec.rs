//! "On enter" action, like fzf's `--bind 'enter:execute(...)'`.

use std::fs::File;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use eyre::Result;

/// Command run when Enter is pressed on a file.
///
/// `{}` in the template is replaced by the shell-quoted path (relative to the
/// caller's cwd), `{abs}` by the absolute path. Without any placeholder the
/// path is appended as the last argument.
#[derive(Debug, Clone)]
pub struct OnEnter {
    pub template: String,
    /// Return to the picker afterwards instead of exiting.
    pub keep_open: bool,
    /// Search root (absolute) and the DIR argument as given by the user.
    pub root: PathBuf,
    pub dir: PathBuf,
}

impl OnEnter {
    pub fn rel(&self, p: &str) -> String {
        self.dir
            .join(p)
            .to_string_lossy()
            .trim_start_matches("./")
            .to_owned()
    }

    pub fn abs(&self, p: &str) -> String {
        self.root.join(p).to_string_lossy().into_owned()
    }

    /// Shell command line for the file `p` (path relative to the search root).
    pub fn command(&self, p: &str) -> String {
        let (rel, abs) = (quote(&self.rel(p)), quote(&self.abs(p)));
        let t = &self.template;
        if t.contains("{}") || t.contains("{abs}") {
            t.replace("{abs}", &abs).replace("{}", &rel)
        } else {
            format!("{t} {rel}")
        }
    }
}

/// POSIX single-quote escaping.
pub fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Run `cmd` through `sh -c` with stdio attached to the tty (so editors work
/// even when our stdout is captured). Returns the exit code.
pub fn run(cmd: &str, tty: &File) -> Result<i32> {
    let status = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdin(Stdio::from(tty.try_clone()?))
        .stdout(Stdio::from(tty.try_clone()?))
        .stderr(Stdio::from(tty.try_clone()?))
        .status()?;
    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oe(t: &str) -> OnEnter {
        OnEnter {
            template: t.into(),
            keep_open: false,
            root: "/repo/sub".into(),
            dir: "sub".into(),
        }
    }

    #[test]
    fn placeholders() {
        assert_eq!(oe("nvim {}").command("src/a.rs"), "nvim 'sub/src/a.rs'");
        assert_eq!(oe("nvim").command("a.rs"), "nvim 'sub/a.rs'");
        assert_eq!(
            oe("cp {abs} /tmp/{}").command("a"),
            "cp '/repo/sub/a' /tmp/'sub/a'"
        );
    }

    #[test]
    fn quoting_is_safe() {
        assert_eq!(quote("it's $HOME"), r#"'it'\''s $HOME'"#);
        let out = Command::new("sh")
            .arg("-c")
            .arg(format!("printf %s {}", quote("a b'; echo pwned")))
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "a b'; echo pwned");
    }

    #[test]
    fn dot_dir_has_no_prefix() {
        let mut o = oe("x");
        o.dir = ".".into();
        assert_eq!(o.rel("a.rs"), "a.rs");
    }
}
