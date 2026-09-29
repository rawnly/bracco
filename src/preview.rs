//! `--preview CMD`, like fzf: run a command for the highlighted file and show
//! its output in a pane. Commands run on a worker thread so typing and moving
//! never wait on them; superseded runs are killed, output is size- and
//! time-limited, and only SGR colors survive from the command's output.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eyre::{Result, bail};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::exec::OnEnter;

const MAX_BYTES: usize = 256 * 1024;
const MAX_LINES: usize = 3000;
const TIMEOUT: Duration = Duration::from_secs(3);
const DEBOUNCE: Duration = Duration::from_millis(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Right,
    Left,
    Up,
    Down,
}

/// Parsed `--preview-window`: `[right|left|up|down][:N%][:hidden]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub position: Position,
    pub percent: u16,
    pub hidden: bool,
}

impl Window {
    pub fn parse(spec: &str) -> Result<Self> {
        let mut w = Window { position: Position::Right, percent: 50, hidden: false };
        for tok in spec.split(':').map(str::trim).filter(|t| !t.is_empty()) {
            match tok {
                "right" => w.position = Position::Right,
                "left" => w.position = Position::Left,
                "up" | "top" => w.position = Position::Up,
                "down" | "bottom" => w.position = Position::Down,
                "hidden" => w.hidden = true,
                t => match t.strip_suffix('%').and_then(|n| n.parse::<u16>().ok()) {
                    Some(n) if (10..=90).contains(&n) => w.percent = n,
                    _ => bail!(
                        "invalid --preview-window token `{t}` \
                         (right|left|up|down, N% between 10 and 90, hidden)"
                    ),
                },
            }
        }
        Ok(w)
    }

    /// Split `area` into (list, preview). No preview when there is no room.
    pub fn split(&self, area: Rect) -> (Rect, Option<Rect>) {
        let horizontal = matches!(self.position, Position::Right | Position::Left);
        if (horizontal && area.width < 70) || (!horizontal && area.height < 14) {
            return (area, None);
        }
        let p = Constraint::Percentage(self.percent);
        let rest = Constraint::Min(1);
        match self.position {
            Position::Right => {
                let [l, r] = Layout::horizontal([rest, p]).areas(area);
                (l, Some(r))
            }
            Position::Left => {
                let [l, r] = Layout::horizontal([p, rest]).areas(area);
                (r, Some(l))
            }
            Position::Down => {
                let [t, b] = Layout::vertical([rest, p]).areas(area);
                (t, Some(b))
            }
            Position::Up => {
                let [t, b] = Layout::vertical([p, rest]).areas(area);
                (b, Some(t))
            }
        }
    }
}

struct Req {
    id: u64,
    cmd: String,
    cols: u16,
    rows: u16,
}

struct Resp {
    id: u64,
    lines: Vec<Line<'static>>,
}

pub struct Preview {
    pub window: Window,
    pub visible: bool,
    template: OnEnter,
    tx: Sender<Req>,
    rx: Receiver<Resp>,
    /// (path, cols, rows) of the latest request.
    requested: Option<(String, u16, u16)>,
    latest_id: u64,
    pub lines: Vec<Line<'static>>,
    pub loading: bool,
    pub scroll: u16,
    height: u16,
}

impl Preview {
    pub fn new(template: OnEnter, window: Window) -> Self {
        let (tx, worker_rx) = channel::<Req>();
        let (worker_tx, rx) = channel::<Resp>();
        std::thread::spawn(move || worker(worker_rx, worker_tx));
        Self {
            window,
            visible: !window.hidden,
            template,
            tx,
            rx,
            requested: None,
            latest_id: 0,
            lines: Vec::new(),
            loading: false,
            scroll: 0,
            height: 0,
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Ask for the preview of `path` at the given pane size (no-op if unchanged).
    pub fn request(&mut self, path: Option<&str>, cols: u16, rows: u16) {
        self.height = rows;
        let Some(path) = path else {
            self.requested = None;
            self.lines.clear();
            self.loading = false;
            return;
        };
        if self.requested.as_ref().is_some_and(|(p, c, r)| p == path && *c == cols && *r == rows) {
            return;
        }
        if self.requested.as_ref().is_none_or(|(p, _, _)| p != path) {
            self.scroll = 0;
        }
        self.requested = Some((path.to_owned(), cols, rows));
        self.latest_id += 1;
        self.loading = true;
        let _ = self.tx.send(Req {
            id: self.latest_id,
            cmd: self.template.command(path),
            cols,
            rows,
        });
    }

    /// Apply finished previews. Returns true if the pane content changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(resp) = self.rx.try_recv() {
            if resp.id == self.latest_id {
                self.lines = resp.lines;
                self.loading = false;
                self.scroll = self.scroll.min(self.max_scroll());
                changed = true;
            }
        }
        changed
    }

    fn max_scroll(&self) -> u16 {
        (self.lines.len() as u16).saturating_sub(self.height)
    }

    pub fn scroll_by(&mut self, delta: i32) {
        let s = (self.scroll as i32 + delta).clamp(0, self.max_scroll() as i32);
        self.scroll = s as u16;
    }

    pub fn page(&self) -> i32 {
        (self.height.max(2) - 1) as i32
    }
}

fn worker(rx: Receiver<Req>, tx: Sender<Resp>) {
    let mut next: Option<Req> = None;
    loop {
        let mut req = match next.take() {
            Some(r) => r,
            None => match rx.recv() {
                Ok(r) => r,
                Err(_) => return,
            },
        };
        // Debounce: holding ↓ must not spawn a process per row.
        std::thread::sleep(DEBOUNCE);
        while let Ok(r) = rx.try_recv() {
            req = r;
        }
        let Some(bytes) = run(&req, &rx, &mut next) else { continue };
        let lines = parse_ansi(&String::from_utf8_lossy(&bytes));
        if tx.send(Resp { id: req.id, lines }).is_err() {
            return;
        }
    }
}

/// Preview output is captured through a pipe, and most tools then turn colors
/// off. Ask them to keep colors on (unless the user opted out with NO_COLOR).
/// Tools that only colorize on a tty *or* with a flag need the flag, e.g.
/// `glow -s dark -w $FFF_PREVIEW_COLUMNS {}`.
fn force_color(cmd: &mut Command) {
    if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return;
    }
    cmd.env("CLICOLOR_FORCE", "1").env("FORCE_COLOR", "1");
    // bat has no env switch, but reads extra options from BAT_OPTS; a
    // --color flag on the command line still wins over it.
    let bat_opts = match std::env::var("BAT_OPTS") {
        Ok(v) if !v.is_empty() => format!("{v} --color=always"),
        _ => "--color=always".to_owned(),
    };
    cmd.env("BAT_OPTS", bat_opts);
}

/// Run one preview command. `None` if it was superseded by a newer request.
fn run(req: &Req, rx: &Receiver<Req>, next: &mut Option<Req>) -> Option<Vec<u8>> {
    let cols = req.cols.to_string();
    let rows = req.rows.to_string();
    let mut command = Command::new("sh");
    force_color(&mut command);
    let mut child = match command
        .arg("-c")
        .arg(format!("exec 2>&1; {}", req.cmd))
        .env("FFF_PREVIEW_COLUMNS", &cols)
        .env("FFF_PREVIEW_LINES", &rows)
        .env("FZF_PREVIEW_COLUMNS", &cols)
        .env("FZF_PREVIEW_LINES", &rows)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return Some(format!("preview failed: {e}").into_bytes()),
    };

    // Shared buffer instead of join(): a grandchild holding the pipe open
    // after a kill must not be able to hang us.
    let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let mut stdout = child.stdout.take().expect("piped stdout");
    let reader_buf = Arc::clone(&buf);
    std::thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        while let Ok(n) = stdout.read(&mut chunk) {
            let mut b = reader_buf.lock().unwrap();
            if n == 0 || b.len() >= MAX_BYTES {
                break;
            }
            b.extend_from_slice(&chunk[..n]);
        }
    });

    let start = Instant::now();
    let mut note = "";
    loop {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => break,
            Ok(None) => {}
        }
        let mut newer = None;
        while let Ok(r) = rx.try_recv() {
            newer = Some(r);
        }
        if newer.is_some() {
            *next = newer;
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        if start.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            note = "\n[preview timed out]";
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // Let the reader drain what the (now finished) child wrote.
    std::thread::sleep(Duration::from_millis(15));
    let mut out = buf.lock().unwrap().clone();
    out.extend_from_slice(note.as_bytes());
    Some(out)
}

/// Text → styled lines. Keeps SGR (colors/bold/...) and drops every other
/// escape sequence and control character: command output is untrusted and
/// must not move the cursor, retitle the window, etc.
pub fn parse_ansi(input: &str) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut text = String::new();
    let mut style = Style::new();

    fn flush(text: &mut String, spans: &mut Vec<Span<'static>>, style: Style) {
        if !text.is_empty() {
            spans.push(Span::styled(std::mem::take(text), style));
        }
    }

    let mut it = input.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\x1b' => match it.peek().copied() {
                Some('[') => {
                    it.next();
                    let mut params = String::new();
                    let mut fin = '\0';
                    for ch in it.by_ref() {
                        if ('\x40'..='\x7e').contains(&ch) {
                            fin = ch;
                            break;
                        }
                        params.push(ch);
                    }
                    if fin == 'm' {
                        flush(&mut text, &mut spans, style);
                        style = apply_sgr(style, &params);
                    }
                }
                Some(']') => {
                    // OSC: until BEL or ESC \
                    it.next();
                    while let Some(ch) = it.next() {
                        if ch == '\x07' {
                            break;
                        }
                        if ch == '\x1b' {
                            it.next();
                            break;
                        }
                    }
                }
                Some(_) => {
                    it.next();
                }
                None => {}
            },
            '\n' => {
                flush(&mut text, &mut spans, style);
                lines.push(Line::from(std::mem::take(&mut spans)));
                if lines.len() >= MAX_LINES {
                    return lines;
                }
            }
            '\t' => text.push_str("    "),
            c if c.is_control() => {}
            c => text.push(c),
        }
    }
    flush(&mut text, &mut spans, style);
    if !spans.is_empty() {
        lines.push(Line::from(spans));
    }
    lines
}

/// Flatten SGR parameters to the `;` form. Handles the ITU colon syntax that
/// some tools emit (`38:2::r:g:b`, `38:2:r:g:b`, `38:5:n`, `4:3`).
fn sgr_numbers(params: &str) -> Vec<u16> {
    if params.is_empty() {
        return vec![0];
    }
    let mut out = Vec::new();
    for group in params.split(';') {
        if !group.contains(':') {
            out.push(group.parse().unwrap_or(0));
            continue;
        }
        let p: Vec<u16> = group.split(':').map(|x| x.parse().unwrap_or(0)).collect();
        match (p[0], p.get(1)) {
            (38 | 48, Some(2)) if p.len() >= 5 => {
                out.extend([p[0], 2, p[p.len() - 3], p[p.len() - 2], p[p.len() - 1]]);
            }
            (38 | 48, Some(5)) if p.len() >= 3 => out.extend([p[0], 5, p[2]]),
            (4, Some(0)) => out.push(24),
            (first, _) => out.push(first),
        }
    }
    out
}

fn apply_sgr(mut style: Style, params: &str) -> Style {
    let nums = sgr_numbers(params);
    let mut i = 0;
    while i < nums.len() {
        let n = nums[i];
        match n {
            0 => style = Style::new(),
            1 => style = style.add_modifier(Modifier::BOLD),
            2 => style = style.add_modifier(Modifier::DIM),
            3 => style = style.add_modifier(Modifier::ITALIC),
            4 => style = style.add_modifier(Modifier::UNDERLINED),
            7 => style = style.add_modifier(Modifier::REVERSED),
            22 => style = style.remove_modifier(Modifier::BOLD | Modifier::DIM),
            23 => style = style.remove_modifier(Modifier::ITALIC),
            24 => style = style.remove_modifier(Modifier::UNDERLINED),
            27 => style = style.remove_modifier(Modifier::REVERSED),
            30..=37 => style = style.fg(basic(n - 30)),
            90..=97 => style = style.fg(bright(n - 90)),
            40..=47 => style = style.bg(basic(n - 40)),
            100..=107 => style = style.bg(bright(n - 100)),
            39 => style.fg = None,
            49 => style.bg = None,
            38 | 48 => {
                let color = match nums.get(i + 1) {
                    Some(5) => nums.get(i + 2).map(|&v| {
                        i += 2;
                        Color::Indexed(v.min(255) as u8)
                    }),
                    Some(2) if i + 4 < nums.len() => {
                        let c = Color::Rgb(
                            nums[i + 2].min(255) as u8,
                            nums[i + 3].min(255) as u8,
                            nums[i + 4].min(255) as u8,
                        );
                        i += 4;
                        Some(c)
                    }
                    _ => None,
                };
                if let Some(c) = color {
                    style = if n == 38 { style.fg(c) } else { style.bg(c) };
                }
            }
            _ => {}
        }
        i += 1;
    }
    style
}

fn basic(n: u16) -> Color {
    [
        Color::Black,
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
        Color::Gray,
    ][n as usize % 8]
}

fn bright(n: u16) -> Color {
    [
        Color::DarkGray,
        Color::LightRed,
        Color::LightGreen,
        Color::LightYellow,
        Color::LightBlue,
        Color::LightMagenta,
        Color::LightCyan,
        Color::White,
    ][n as usize % 8]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn window_spec() {
        let w = Window::parse("down:40%:hidden").unwrap();
        assert_eq!((w.position, w.percent, w.hidden), (Position::Down, 40, true));
        assert_eq!(Window::parse("right:50%").unwrap().percent, 50);
        assert!(Window::parse("sideways").is_err());
        assert!(Window::parse("5%").is_err());
    }

    #[test]
    fn split_collapses_when_narrow() {
        let w = Window::parse("right:50%").unwrap();
        assert!(w.split(Rect::new(0, 0, 60, 30)).1.is_none());
        let (list, pv) = w.split(Rect::new(0, 0, 100, 30));
        assert_eq!(list.width + pv.unwrap().width, 100);
    }

    #[test]
    fn ansi_keeps_colors_drops_everything_else() {
        let l = parse_ansi("\x1b[31mred\x1b[0m plain\n\x1b]0;evil title\x07\x1b[2J\x1b[Hx\ry\tz\x07");
        assert_eq!(l.len(), 2);
        assert_eq!(plain(&l[0]), "red plain");
        assert_eq!(l[0].spans[0].style.fg, Some(Color::Red));
        assert_eq!(plain(&l[1]), "xy    z");
    }

    #[test]
    fn sgr_colon_syntax_and_bare_reset() {
        let l = parse_ansi("\x1b[38:2::10:20:30ma\x1b[48:5:9mb\x1b[mc\x1b[4:3md");
        assert_eq!(l[0].spans[0].style.fg, Some(Color::Rgb(10, 20, 30)));
        assert_eq!(l[0].spans[1].style.bg, Some(Color::Indexed(9)));
        assert_eq!(l[0].spans[1].style.fg, Some(Color::Rgb(10, 20, 30)));
        assert_eq!(l[0].spans[2].style, Style::new());
        assert!(l[0].spans[3].style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn sgr_256_and_rgb() {
        let l = parse_ansi("\x1b[38;5;208ma\x1b[38;2;1;2;3;1mb");
        assert_eq!(l[0].spans[0].style.fg, Some(Color::Indexed(208)));
        assert_eq!(l[0].spans[1].style.fg, Some(Color::Rgb(1, 2, 3)));
    }
}
