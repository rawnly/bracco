mod filters;
mod finder;
mod icons;
mod tui;

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use eyre::{Result, bail};

use crate::filters::{Filters, StatusFilter};
use tracing::{debug, info};
use usage::Cli;

/// Fuzzy file picker: git-modified files first, respects .gitignore.
///
/// The interactive UI is drawn on /dev/tty; the selected path goes to stdout.
/// Exit code: 0 selected, 1 cancelled / no match, 2 error.
#[derive(Debug, Cli)]
#[usage(bin = "fff-picker", version = env!("CARGO_PKG_VERSION"), unknown_flags = "error", completion)]
struct Args {
    /// Initial query (interactive) or the query to run (--list)
    #[usage(short, long)]
    query: Option<String>,

    /// Non-interactive: print matches to stdout and exit
    #[usage(short, long)]
    list: bool,

    /// Max results for --list
    #[usage(short = 'n', long, default = "100")]
    limit: usize,

    /// Print absolute paths
    #[usage(short, long)]
    absolute: bool,

    /// Separate output paths with NUL instead of newline
    #[usage(short = '0', long)]
    print0: bool,

    /// Write logs to this file (level via RUST_LOG, default fff_picker=debug)
    #[usage(long, env = "FFF_PICKER_LOG_FILE", value_hint = usage::ValueHint::FilePath)]
    log_file: Option<PathBuf>,

    /// Only files with this git status: all, changed, staged, unstaged, untracked, clean
    #[usage(long, choices("all", "changed", "staged", "unstaged", "untracked", "clean"))]
    status: Option<String>,

    /// Shortcut for --status changed
    #[usage(short, long)]
    changed: bool,

    /// Only files with this extension (repeatable, or comma-separated)
    #[usage(short, long)]
    ext: Vec<String>,

    /// Exclude paths matching this glob (repeatable): `node_modules`, `*.lock`, `src/gen`
    #[usage(short = 'x', long)]
    exclude: Vec<String>,

    /// Directory to search
    #[usage(default = ".", value_hint = usage::ValueHint::DirPath)]
    dir: PathBuf,

    #[usage(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, usage::Subcommands)]
enum Command {
    /// Print a shell completion script
    Completions(Completions),
    /// Print the usage spec (input for `usage generate manpage|markdown`)
    Spec(Spec),
}

#[derive(Debug, usage::Args)]
struct Completions {
    /// Target shell
    #[usage(choices("bash", "zsh", "fish", "elvish", "nu", "powershell"))]
    shell: String,
}

#[derive(Debug, usage::Args)]
struct Spec {}

/// Logs go to a file only: stdout carries the result, /dev/tty carries the UI.
fn init_logging(path: Option<&PathBuf>) -> Result<()> {
    let Some(path) = path else { return Ok(()) };
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "fff_picker=debug".into());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::sync::Mutex::new(file))
        .with_ansi(false)
        .init();
    Ok(())
}

fn generate(cmd: &Command) -> Result<()> {
    let text = match cmd {
        Command::Spec(_) => Args::to_kdl(),
        Command::Completions(c) => {
            let Some(shell) = usage::complete::Shell::from_name(&c.shell) else {
                bail!("unknown shell `{}`", c.shell);
            };
            Args::completion_script(shell)
        }
    };
    print!("{text}");
    Ok(())
}

fn run(args: Args) -> Result<bool> {
    if let Some(cmd) = &args.command {
        generate(cmd)?;
        return Ok(true);
    }
    init_logging(args.log_file.as_ref())?;
    debug!(?args, "starting");

    let root = std::fs::canonicalize(&args.dir)?;
    let finder = finder::Finder::open(&root)?;
    let query = args.query.as_deref().unwrap_or("");
    let status = match (args.status.as_deref(), args.changed) {
        (Some(s), _) => StatusFilter::parse(s).ok_or_else(|| {
            eyre::eyre!("invalid --status `{s}` (all, changed, staged, unstaged, untracked, clean)")
        })?,
        (None, true) => StatusFilter::Changed,
        (None, false) => StatusFilter::All,
    };
    let filters = Filters::new(status, &args.ext, &args.exclude);
    let sep: &[u8] = if args.print0 { b"\0" } else { b"\n" };
    // Relative output is relative to the caller's cwd, not to DIR.
    let fmt = |p: &str| {
        if args.absolute {
            root.join(p).to_string_lossy().into_owned()
        } else {
            args.dir.join(p).to_string_lossy().trim_start_matches("./").to_owned()
        }
    };

    let mut out = std::io::stdout().lock();
    let write = |out: &mut std::io::StdoutLock, p: &str| -> std::io::Result<()> {
        out.write_all(fmt(p).as_bytes())?;
        out.write_all(sep)
    };

    if args.list {
        let entries = finder.search(query, &filters, args.limit).entries;
        for e in &entries {
            match write(&mut out, &e.path) {
                Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => return Ok(true),
                r => r?,
            }
        }
        out.flush()?;
        return Ok(!entries.is_empty());
    }

    let Ok(tty) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
    else {
        bail!("no controlling terminal; use --list for non-interactive output");
    };

    match tui::run(tty, &finder, query, filters)? {
        Some(p) => {
            info!(path = %p, "selected");
            write(&mut out, &p)?;
            out.flush()?;
            Ok(true)
        }
        None => {
            info!("cancelled");
            Ok(false)
        }
    }
}

fn main() -> ExitCode {
    // Handles --help / --version / usage errors itself.
    let args = Args::parse();
    match run(args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            tracing::error!(error = %e, "failed");
            eprintln!("fff-picker: {e}");
            ExitCode::from(2)
        }
    }
}
