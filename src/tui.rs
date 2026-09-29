use std::io::Write;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use eyre::Result;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use crate::exec::{self, OnEnter};
use crate::filters::{Filters, StatusFilter};
use crate::finder::{Entry, Finder, GitState};
use crate::icons::icon_for;
use crate::preview::{Position, Preview};

const LIMIT: usize = 2000;

/// Restores the terminal on drop (normal exit, error, or panic unwind).
struct TermGuard(std::fs::File);

impl Drop for TermGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.0, LeaveAlternateScreen);
        let _ = self.0.flush();
    }
}

pub enum Outcome {
    Cancelled,
    /// Enter pressed, no on-enter action configured: the chosen path.
    Selected(String),
    /// The on-enter command ran (and we exit): its exit code.
    Executed(i32),
}

/// Run the interactive picker on the given tty so stdout stays clean for the
/// selected path.
pub fn run(
    tty: std::fs::File,
    finder: &mut Finder,
    initial_query: &str,
    filters: Filters,
    on_enter: Option<&OnEnter>,
    preview: Option<Preview>,
    vim: bool,
) -> Result<Outcome> {
    enable_raw_mode()?;
    let mut out = tty.try_clone()?;
    let _guard = TermGuard(tty.try_clone()?);
    execute!(out, EnterAlternateScreen)?;

    // Make panics readable: restore first, then run the previous hook.
    let prev = std::panic::take_hook();
    let panic_tty = tty.try_clone()?;
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(&panic_tty, LeaveAlternateScreen);
        prev(info);
    }));

    let mut terminal = Terminal::new(CrosstermBackend::new(tty.try_clone()?))?;
    event_loop(
        &mut terminal,
        &tty,
        finder,
        initial_query,
        filters,
        on_enter,
        preview,
        vim,
    )
}

#[allow(clippy::too_many_arguments)]
fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::fs::File>>,
    tty: &std::fs::File,
    finder: &mut Finder,
    initial_query: &str,
    mut filters: Filters,
    on_enter: Option<&OnEnter>,
    mut preview: Option<Preview>,
    vim: bool,
) -> Result<Outcome> {
    // With --vim the picker starts in normal mode; `/`, `i` or `a` enter the
    // search (insert) mode and esc goes back. Without --vim it is always insert.
    let mut insert = !vim;
    let mut show_help = false;
    let mut help_scroll: u16 = 0;
    let mut query = initial_query.to_string();
    // Byte offset of the text cursor in `query` (always on a char boundary).
    let mut cursor = query.len();
    let mut res = finder.search(&query, &filters, LIMIT);
    let mut entries: Vec<Entry> = std::mem::take(&mut res.entries);
    let mut total = res.total;
    let mut state = ListState::default().with_selected(Some(0));

    loop {
        // Keep the preview in sync with the highlighted file and pane size.
        if let Some(pv) = preview.as_mut().filter(|p| p.visible) {
            let [_, list, _] = main_areas(terminal.size()?.into());
            let inner = pv
                .window
                .split(list)
                .1
                .map(|r| pane_block(pv.window.position).inner(r));
            match inner {
                Some(r) => {
                    let sel = entries
                        .get(state.selected().unwrap_or(0))
                        .map(|e| e.path.as_str());
                    pv.request(sel, r.width, r.height);
                }
                None => pv.request(None, 0, 0),
            }
        }

        terminal.draw(|f| {
            let [input, list_full, status] = main_areas(f.area());
            let (list, preview_area) = match preview.as_ref().filter(|p| p.visible && !show_help) {
                Some(pv) => pv.window.split(list_full),
                None => (list_full, None),
            };

            // Input row: prompt + query on the left, active filters right-aligned.
            const MIN_INPUT: u16 = 12;
            let chips = filter_chips(&filters, input.width.saturating_sub(MIN_INPUT));
            let chips_w = chips.width() as u16;
            let [text_area, chips_area] =
                Layout::horizontal([Constraint::Min(1), Constraint::Length(chips_w)]).areas(input);

            let prompt = Span::styled(
                "❯ ",
                Style::new()
                    .fg(if insert { Color::Cyan } else { Color::DarkGray })
                    .add_modifier(Modifier::BOLD),
            );
            let text = if query.is_empty() {
                let hint = if insert {
                    "type to search…"
                } else {
                    "press / to search"
                };
                Line::from(vec![
                    prompt,
                    Span::styled(hint, Style::new().fg(Color::DarkGray)),
                ])
            } else {
                Line::from(vec![prompt, Span::raw(query.as_str())])
            };
            f.render_widget(Paragraph::new(text), text_area);
            f.render_widget(
                Paragraph::new(chips).alignment(Alignment::Right),
                chips_area,
            );

            if insert {
                f.set_cursor_position((
                    (text_area.x + 2 + Line::raw(&query[..cursor]).width() as u16)
                        .min(text_area.right().saturating_sub(1)),
                    text_area.y,
                ));
            }

            let items: Vec<ListItem> = entries
                .iter()
                .map(|e| {
                    let st = e.state;
                    let path_color = path_color(st);
                    let (icon, icon_color) = icon_for(&e.path);
                    let icon_color = if path_color == Color::Reset {
                        icon_color
                    } else {
                        path_color
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(st.x.to_string(), Style::new().fg(index_color(st))),
                        Span::styled(st.y.to_string(), Style::new().fg(worktree_color(st))),
                        Span::raw(" "),
                        Span::styled(format!("{icon} "), Style::new().fg(icon_color)),
                        Span::styled(e.path.as_str(), Style::new().fg(path_color)),
                    ]))
                })
                .collect();
            if let (Some(pv), Some(area)) = (preview.as_ref(), preview_area) {
                let block = pane_block(pv.window.position);
                let inner = block.inner(area);
                f.render_widget(block, area);
                if pv.lines.is_empty() && pv.loading {
                    f.render_widget(
                        Paragraph::new(Span::styled("loading…", Style::new().fg(Color::DarkGray))),
                        inner,
                    );
                } else {
                    f.render_widget(
                        Paragraph::new(pv.lines.clone()).scroll((pv.scroll, 0)),
                        inner,
                    );
                }
            }

            if show_help {
                let lines = help_lines(&filters, &query);
                let max = (lines.len() as u16).saturating_sub(list_full.height);
                f.render_widget(
                    Paragraph::new(lines).scroll((help_scroll.min(max), 0)),
                    list,
                );
            } else {
                f.render_stateful_widget(
                    List::new(items)
                        .highlight_symbol("▌")
                        // Theme-native ANSI color (follows the terminal palette); never
                        // REVERSED, which would turn per-span fg colors into blocks.
                        .highlight_style(Style::new().bg(Color::DarkGray)),
                    list,
                    &mut state,
                );
            }

            f.render_widget(
                Paragraph::new(if show_help {
                    "esc / q / ? close  ·  ↑↓ PgUp PgDn scroll".to_string()
                } else if vim && !insert {
                    format!(
                        "NORMAL  {}/{} files  ·  j/k move · J/K preview · / search · ? help",
                        entries.len(),
                        total
                    )
                } else if vim {
                    format!(
                        "INSERT  {}/{} files  ·  esc normal mode · enter open · ? help",
                        entries.len(),
                        total
                    )
                } else {
                    format!(
                        "{}/{} files  ·  tab filter · ^g ^s ^a · ? help",
                        entries.len(),
                        total
                    )
                })
                .style(Style::new().fg(Color::DarkGray)),
                status,
            );
        })?;

        // Wait for a key, or redraw when a preview finishes / on resize.
        let key = loop {
            if preview.as_mut().is_some_and(|p| p.poll()) {
                break None;
            }
            if event::poll(std::time::Duration::from_millis(30))? {
                match event::read()? {
                    Event::Key(k) => break Some(k),
                    Event::Resize(..) => break None,
                    _ => {}
                }
            }
        };
        let Some(key) = key else { continue };
        if key.kind == KeyEventKind::Release {
            continue;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let sel = state.selected().unwrap_or(0);
        let last = entries.len().saturating_sub(1);
        let mut changed = false;

        if show_help {
            let page = terminal.size()?.height.saturating_sub(2);
            match key.code {
                KeyCode::Esc
                | KeyCode::Char('q')
                | KeyCode::Char('?')
                | KeyCode::F(1)
                | KeyCode::Enter => {
                    show_help = false;
                    help_scroll = 0;
                }
                KeyCode::Char('c') if ctrl => return Ok(Outcome::Cancelled),
                KeyCode::Up | KeyCode::Char('k') => help_scroll = help_scroll.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => help_scroll = help_scroll.saturating_add(1),
                KeyCode::PageUp => help_scroll = help_scroll.saturating_sub(page),
                KeyCode::PageDown => help_scroll = help_scroll.saturating_add(page),
                KeyCode::Home => help_scroll = 0,
                _ => {}
            }
            continue;
        }

        let shift = key.modifiers.contains(KeyModifiers::SHIFT);

        if vim && !insert && !ctrl {
            let page = preview.as_ref().map_or(0, |p| p.page());
            match key.code {
                KeyCode::Char('j') => state.select(Some((sel + 1).min(last))),
                KeyCode::Char('k') => state.select(Some(sel.saturating_sub(1))),
                KeyCode::Char('J') => {
                    if let Some(pv) = preview.as_mut() {
                        pv.scroll_by(1);
                    }
                }
                KeyCode::Char('K') => {
                    if let Some(pv) = preview.as_mut() {
                        pv.scroll_by(-1);
                    }
                }
                KeyCode::Char('g') => state.select(Some(0)),
                KeyCode::Char('G') => state.select(Some(last)),
                KeyCode::Char('/') | KeyCode::Char('i') | KeyCode::Char('a') => insert = true,
                KeyCode::Char('q') => return Ok(Outcome::Cancelled),
                KeyCode::Char('?') => show_help = true,
                KeyCode::Char('d') => state.select(Some((sel + 10).min(last))),
                KeyCode::Char('u') => state.select(Some(sel.saturating_sub(10))),
                KeyCode::Char('f') => {
                    if let Some(pv) = preview.as_mut() {
                        pv.scroll_by(page);
                    }
                }
                KeyCode::Char('b') => {
                    if let Some(pv) = preview.as_mut() {
                        pv.scroll_by(-page);
                    }
                }
                // Plain characters never edit the query outside insert mode.
                KeyCode::Char(_) => {}
                _ => {}
            }
            if matches!(key.code, KeyCode::Char(_)) {
                continue;
            }
        }

        match key.code {
            KeyCode::Esc if vim && insert => insert = false,
            KeyCode::Esc => return Ok(Outcome::Cancelled),
            KeyCode::Char('o') if ctrl => {
                if let Some(pv) = preview.as_mut() {
                    pv.toggle();
                }
            }
            KeyCode::Char('d') if ctrl => {
                if let Some(pv) = preview.as_mut() {
                    pv.scroll_by(pv.page() / 2);
                }
            }
            KeyCode::Char('u') if ctrl => {
                if let Some(pv) = preview.as_mut() {
                    pv.scroll_by(-(pv.page() / 2));
                }
            }
            KeyCode::Up if shift => {
                if let Some(pv) = preview.as_mut() {
                    pv.scroll_by(-1);
                }
            }
            KeyCode::Down if shift => {
                if let Some(pv) = preview.as_mut() {
                    pv.scroll_by(1);
                }
            }
            KeyCode::PageUp if shift => {
                if let Some(pv) = preview.as_mut() {
                    pv.scroll_by(-pv.page());
                }
            }
            KeyCode::PageDown if shift => {
                if let Some(pv) = preview.as_mut() {
                    pv.scroll_by(pv.page());
                }
            }
            KeyCode::Char('c') if ctrl => return Ok(Outcome::Cancelled),
            KeyCode::F(1) => show_help = true,
            KeyCode::Char('?') if query.is_empty() && !vim => show_help = true,
            KeyCode::Tab => {
                filters.status = filters.status.next();
                changed = true;
            }
            KeyCode::BackTab => {
                // reverse cycle = 4 forward steps in a 5-cycle
                for _ in 0..4 {
                    filters.status = filters.status.next();
                }
                changed = true;
            }
            KeyCode::Char('g') if ctrl => {
                filters.status = filters.status.toggle(StatusFilter::Changed);
                changed = true;
            }
            KeyCode::Char('s') if ctrl => {
                filters.status = filters.status.toggle(StatusFilter::Staged);
                changed = true;
            }
            KeyCode::Char('a') if ctrl => {
                filters.status = filters.status.toggle(StatusFilter::Unstaged);
                changed = true;
            }
            KeyCode::Char('t') if ctrl => {
                filters.status = filters.status.toggle(StatusFilter::Untracked);
                changed = true;
            }
            KeyCode::Enter => {
                if let Some(e) = entries.get(sel) {
                    let Some(oe) = on_enter else {
                        return Ok(Outcome::Selected(e.path.clone()));
                    };
                    // Hand the terminal to the command (editor, pager, ...).
                    let cmd = oe.command(&e.path);
                    tracing::info!(%cmd, "on-enter");
                    let mut out = tty.try_clone()?;
                    disable_raw_mode()?;
                    execute!(out, LeaveAlternateScreen)?;
                    let code = exec::run(&cmd, tty);
                    enable_raw_mode()?;
                    execute!(out, EnterAlternateScreen)?;
                    // Fresh terminal = full redraw (Terminal::clear would query the
                    // cursor position and wait for the emulator's reply).
                    *terminal = Terminal::new(CrosstermBackend::new(tty.try_clone()?))?;
                    let code = code?;
                    tracing::info!(code, "on-enter finished");
                    if !oe.keep_open {
                        return Ok(Outcome::Executed(code));
                    }
                    // Back from the command: it may have edited / created /
                    // deleted files, so rescan and keep query, filters and the
                    // highlighted file.
                    let keep = e.path.clone();
                    match finder.refresh() {
                        Ok(()) => {
                            let r = finder.search(&query, &filters, LIMIT);
                            total = r.total;
                            entries = r.entries;
                            let idx = entries.iter().position(|x| x.path == keep);
                            state.select(Some(
                                idx.unwrap_or(sel.min(entries.len().saturating_sub(1))),
                            ));
                            if let Some(pv) = preview.as_mut() {
                                pv.invalidate();
                            }
                        }
                        Err(err) => tracing::warn!(error = %err, "refresh failed"),
                    }
                }
            }
            KeyCode::Up => state.select(Some(sel.saturating_sub(1))),
            KeyCode::Char('p') | KeyCode::Char('k') if ctrl => {
                state.select(Some(sel.saturating_sub(1)))
            }
            KeyCode::Down => state.select(Some((sel + 1).min(last))),
            KeyCode::Char('n') | KeyCode::Char('j') if ctrl => {
                state.select(Some((sel + 1).min(last)))
            }
            KeyCode::PageUp => state.select(Some(sel.saturating_sub(10))),
            KeyCode::PageDown => state.select(Some((sel + 10).min(last))),
            KeyCode::Left => cursor = prev_boundary(&query, cursor),
            KeyCode::Right => cursor = next_boundary(&query, cursor),
            KeyCode::Home => cursor = 0,
            KeyCode::End => cursor = query.len(),
            KeyCode::Backspace => {
                let start = prev_boundary(&query, cursor);
                changed = start != cursor;
                query.replace_range(start..cursor, "");
                cursor = start;
            }
            KeyCode::Delete => {
                let end = next_boundary(&query, cursor);
                changed = end != cursor;
                query.replace_range(cursor..end, "");
            }
            KeyCode::Char('x') if ctrl => {
                changed = !query.is_empty();
                query.clear();
                cursor = 0;
            }
            KeyCode::Char('w') if ctrl => {
                let t = query[..cursor].trim_end().len();
                let cut = query[..t].rfind(' ').map(|i| i + 1).unwrap_or(0);
                changed = cut != cursor;
                query.replace_range(cut..cursor, "");
                cursor = cut;
            }
            KeyCode::Char(c) if !ctrl => {
                query.insert(cursor, c);
                cursor += c.len_utf8();
                changed = true;
            }
            _ => {}
        }

        if changed {
            let r = finder.search(&query, &filters, LIMIT);
            total = r.total;
            entries = r.entries;
            state.select(Some(0));
        }
    }
}

fn prev_boundary(s: &str, at: usize) -> usize {
    s[..at].char_indices().next_back().map_or(0, |(i, _)| i)
}

fn next_boundary(s: &str, at: usize) -> usize {
    s[at..].chars().next().map_or(at, |c| at + c.len_utf8())
}

// lazygit conventions: staged marker = green, unstaged / untracked marker = red.
fn index_color(st: GitState) -> Color {
    if st.is_conflicted() {
        Color::Red
    } else {
        Color::Green
    }
}

fn worktree_color(_st: GitState) -> Color {
    Color::Red
}

fn path_color(st: GitState) -> Color {
    // lazygit: only staged files get a colored name; unstaged / untracked
    // files keep the default text color (just the status marker is red).
    if st.is_conflicted() {
        Color::Red
    } else if st.has_staged() && st.has_unstaged() {
        Color::Yellow
    } else if st.has_staged() {
        Color::Green
    } else {
        Color::Reset
    }
}

/// (keys, description) rows per section. Keys are what the user presses.
const HELP_SECTIONS: &[(&str, &[(&str, &str)])] = &[
    (
        "Navigate",
        &[
            ("↑ ↓  ^p ^n  ^k ^j", "move selection"),
            ("PgUp PgDn", "jump 10 rows"),
            ("enter", "select file and print its path"),
            ("esc  ^c", "cancel (exit code 1)"),
        ],
    ),
    (
        "Vim mode (--vim)",
        &[
            ("j k  g G", "move selection / jump to top / bottom"),
            ("d u", "jump 10 rows down / up"),
            ("J K  f b", "scroll preview by line / by page"),
            ("/ i a", "enter search (insert mode)"),
            ("esc", "insert → normal; normal → cancel"),
            ("q", "cancel (normal mode)"),
        ],
    ),
    (
        "Preview (with --preview)",
        &[
            ("^o", "show / hide the preview pane"),
            ("^d  ^u", "scroll the preview half a page down / up"),
            ("shift-↑ ↓", "scroll the preview by line"),
            ("shift-PgUp PgDn", "scroll the preview by page"),
        ],
    ),
    (
        "Edit query",
        &[
            ("← →  home end", "move the cursor"),
            ("backspace  del", "delete before / after the cursor"),
            ("^w", "delete the word before the cursor"),
            ("^x", "clear the query"),
        ],
    ),
    (
        "Git filter",
        &[
            (
                "tab  shift-tab",
                "cycle all → changed → staged → unstaged → untracked",
            ),
            ("^g", "changed: anything git reports"),
            ("^s", "staged: has changes in the index"),
            ("^a", "unstaged: modified in the worktree"),
            ("^t", "untracked: new files"),
        ],
    ),
    (
        "Query syntax",
        &[
            ("text", "fuzzy match on the path"),
            ("*.rs", "only this extension"),
            ("/src/", "only inside this directory"),
            ("!test", "exclude matches"),
            ("type:rust", "by file type"),
            (
                "status:modified",
                "also staged, untracked, unmodified (st: g: git:)",
            ),
        ],
    ),
];

fn help_lines(filters: &Filters, query: &str) -> Vec<Line<'static>> {
    const KEY_W: usize = 20;
    let key = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let head = Style::new().add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    let dim = Style::new().fg(Color::DarkGray);

    let mut out: Vec<Line> = Vec::new();
    for (i, (title, rows)) in HELP_SECTIONS.iter().enumerate() {
        if i > 0 {
            out.push(Line::raw(""));
        }
        out.push(Line::from(Span::styled(format!(" {title}"), head)));
        for (keys, desc) in *rows {
            out.push(Line::from(vec![
                Span::styled(format!("   {keys:<KEY_W$}"), key),
                Span::raw(*desc),
            ]));
        }
    }

    // Live state, so the help also answers "why am I seeing this list?".
    out.push(Line::raw(""));
    out.push(Line::from(Span::styled(" Active now", head)));
    let mut now = vec![Span::styled(
        format!("   status: {}", filters.status.label()),
        dim,
    )];
    if !filters.exts.is_empty() {
        now.push(Span::styled(
            format!("   ext: {}", filters.exts.join(", ")),
            dim,
        ));
    }
    if !filters.exclude.is_empty() {
        now.push(Span::styled(
            format!("   exclude: {}", filters.exclude.join(", ")),
            dim,
        ));
    }
    if !query.is_empty() {
        now.push(Span::styled(format!("   query: {query}"), dim));
    }
    out.push(Line::from(now));
    out
}

/// Right-aligned chips for every active filter, e.g. `changed  *.rs  -vendor`.
/// Chips are dropped from the end (excludes first) until they fit `max_width`.
fn filter_chips(filters: &Filters, max_width: u16) -> Line<'static> {
    let chip = |text: String, color: Color| {
        Span::styled(
            format!(" {text} "),
            Style::new().fg(color).add_modifier(Modifier::BOLD),
        )
    };
    let mut chips: Vec<Span<'static>> = Vec::new();
    if filters.status != StatusFilter::All {
        let color = match filters.status {
            StatusFilter::Staged => Color::Green,
            StatusFilter::Unstaged | StatusFilter::Untracked => Color::Red,
            StatusFilter::Changed => Color::Yellow,
            StatusFilter::Clean | StatusFilter::All => Color::Gray,
        };
        chips.push(chip(format!("● {}", filters.status.label()), color));
    }
    if !filters.exts.is_empty() {
        let exts: Vec<String> = filters.exts.iter().map(|e| format!("*.{e}")).collect();
        chips.push(chip(exts.join(" "), Color::Cyan));
    }
    if !filters.exclude.is_empty() {
        let label = match filters.exclude.as_slice() {
            [one] => format!("-{one}"),
            many => format!("-{} excludes", many.len()),
        };
        chips.push(chip(label, Color::Magenta));
    }
    while !chips.is_empty() && Line::from(chips.clone()).width() as u16 > max_width {
        chips.pop();
    }
    Line::from(chips)
}

fn main_areas(area: ratatui::layout::Rect) -> [ratatui::layout::Rect; 3] {
    Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area)
}

/// Thin separator on the side that faces the list.
fn pane_block(pos: Position) -> ratatui::widgets::Block<'static> {
    use ratatui::widgets::{Block, Borders};
    let side = match pos {
        Position::Right => Borders::LEFT,
        Position::Left => Borders::RIGHT,
        Position::Down => Borders::TOP,
        Position::Up => Borders::BOTTOM,
    };
    Block::new()
        .borders(side)
        .border_style(Style::new().fg(Color::DarkGray))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn cursor_boundaries_handle_multibyte() {
        let s = "aé日";
        assert_eq!(next_boundary(s, 0), 1);
        assert_eq!(next_boundary(s, 1), 3);
        assert_eq!(next_boundary(s, 3), s.len());
        assert_eq!(next_boundary(s, s.len()), s.len());
        assert_eq!(prev_boundary(s, s.len()), 3);
        assert_eq!(prev_boundary(s, 3), 1);
        assert_eq!(prev_boundary(s, 0), 0);
    }

    #[test]
    fn chips_show_active_filters_and_drop_when_narrow() {
        let f = Filters::new(StatusFilter::Staged, &["rs".into()], &["vendor".into()]);
        assert_eq!(text(&filter_chips(&f, 80)), " ● staged  *.rs  -vendor ");
        // excludes are dropped first, then extensions, then status
        assert_eq!(text(&filter_chips(&f, 20)), " ● staged  *.rs ");
        assert_eq!(text(&filter_chips(&f, 12)), " ● staged ");
        assert_eq!(text(&filter_chips(&f, 3)), "");
        assert_eq!(filter_chips(&Filters::default(), 80).width(), 0);
    }
}
