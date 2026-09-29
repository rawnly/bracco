//! Nerd Font icons (requires a patched font in the terminal).

use ratatui::style::Color;

const DEFAULT: (&str, Color) = ("\u{f15b}", Color::Gray); // 

/// Icon + color for a file, by name then extension.
pub fn icon_for(path: &str) -> (&'static str, Color) {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name {
        "Cargo.toml" | "Cargo.lock" => return ("\u{e7a8}", Color::Rgb(222, 165, 132)),
        "Dockerfile" | "Containerfile" => return ("\u{f308}", Color::Blue),
        "Makefile" | "justfile" => return ("\u{e779}", Color::Gray),
        "LICENSE" | "LICENSE.md" => return ("\u{f0219}", Color::Yellow),
        ".gitignore" | ".gitattributes" | ".gitmodules" => {
            return ("\u{e702}", Color::Rgb(240, 80, 50));
        }
        _ => {}
    }
    let ext = match name.rsplit_once('.') {
        Some((_, e)) => e.to_ascii_lowercase(),
        None => return DEFAULT,
    };
    match ext.as_str() {
        "rs" => ("\u{e7a8}", Color::Rgb(222, 165, 132)),
        "go" => ("\u{e627}", Color::Cyan),
        "py" => ("\u{e73c}", Color::Yellow),
        "js" | "mjs" | "cjs" => ("\u{e74e}", Color::Yellow),
        "ts" | "tsx" => ("\u{e628}", Color::Blue),
        "jsx" => ("\u{e7ba}", Color::Cyan),
        "json" | "jsonc" => ("\u{e60b}", Color::Yellow),
        "toml" => ("\u{e6b2}", Color::Gray),
        "yaml" | "yml" => ("\u{e6a8}", Color::Gray),
        "md" | "mdx" => ("\u{e73e}", Color::White),
        "html" | "htm" => ("\u{e736}", Color::Rgb(228, 77, 38)),
        "css" | "scss" | "sass" => ("\u{e749}", Color::Blue),
        "sh" | "bash" | "zsh" | "fish" => ("\u{f489}", Color::Green),
        "lua" => ("\u{e620}", Color::Blue),
        "c" | "h" => ("\u{e61e}", Color::Blue),
        "cpp" | "cc" | "hpp" => ("\u{e61d}", Color::Blue),
        "java" => ("\u{e738}", Color::Red),
        "rb" => ("\u{e739}", Color::Red),
        "lock" => ("\u{f023}", Color::Gray),
        "txt" | "log" => ("\u{f15c}", Color::Gray),
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "ico" => ("\u{f1c5}", Color::Magenta),
        "zip" | "tar" | "gz" | "tgz" | "xz" => ("\u{f1c6}", Color::Red),
        "pdf" => ("\u{f1c1}", Color::Red),
        _ => DEFAULT,
    }
}
