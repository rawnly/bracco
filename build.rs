//! Renders the Homebrew formula from `packaging/homebrew/formula.rb.in`.
//!
//! Package metadata (name, version, description, license, repository) comes
//! from Cargo.toml, so the formula can never drift from the crate.
//!
//! Inputs (environment):
//! - `BRACCO_SHA256_<TARGET>`: archive checksum per release target, with the
//!   target triple upper-cased and `-` replaced by `_`, e.g.
//!   `BRACCO_SHA256_AARCH64_APPLE_DARWIN`. Missing ones render as zeros.
//! - `BRACCO_FORMULA_OUT`: also write the formula to this path (the release
//!   workflow uses it; the tap expects `Formula/bracco.rb`).
//!
//! A copy is always written to `$OUT_DIR/<name>.rb`.

use std::path::{Path, PathBuf};
use std::{env, fs};

const TEMPLATE: &str = "packaging/homebrew/formula.rb.in";
const TARGETS: &[&str] = &[
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "aarch64-unknown-linux-gnu",
    "x86_64-unknown-linux-gnu",
];
const ZERO_SHA: &str = "0000000000000000000000000000000000000000000000000000000000000000";

fn var(name: &str) -> String {
    env::var(name).unwrap_or_default()
}

/// `bracco` -> `Bracco`, `my-tool` -> `MyTool` (Homebrew class naming).
fn class_name(name: &str) -> String {
    name.split(['-', '_'])
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut c = s.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect()
}

fn render(template: &str, vars: &[(String, String)]) -> String {
    let mut out = template.to_owned();
    for (key, value) in vars {
        out = out.replace(&format!("{{{{{key}}}}}"), value);
    }
    assert!(
        !out.contains("{{"),
        "unresolved placeholder in {TEMPLATE}: {}",
        out.split("{{")
            .nth(1)
            .unwrap_or("")
            .split("}}")
            .next()
            .unwrap_or("")
    );
    out
}

fn write(path: &Path, contents: &str) {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).expect("create output dir");
    }
    fs::write(path, contents).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

fn main() {
    println!("cargo:rerun-if-changed={TEMPLATE}");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=BRACCO_FORMULA_OUT");

    let name = var("CARGO_PKG_NAME");
    let mut vars = vec![
        ("class".to_owned(), class_name(&name)),
        ("name".to_owned(), name.clone()),
        ("version".to_owned(), var("CARGO_PKG_VERSION")),
        ("description".to_owned(), var("CARGO_PKG_DESCRIPTION")),
        ("license".to_owned(), var("CARGO_PKG_LICENSE")),
        (
            "homepage".to_owned(),
            var("CARGO_PKG_REPOSITORY").trim_end_matches('/').to_owned(),
        ),
    ];
    for target in TARGETS {
        let key = target.replace('-', "_");
        let env_name = format!("BRACCO_SHA256_{}", key.to_uppercase());
        println!("cargo:rerun-if-env-changed={env_name}");
        let sha = env::var(&env_name).unwrap_or_else(|_| ZERO_SHA.to_owned());
        assert!(
            sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit()),
            "{env_name} must be a 64-char hex sha256, got `{sha}`"
        );
        vars.push((format!("sha256_{key}"), sha.to_lowercase()));
    }

    let template = fs::read_to_string(TEMPLATE).unwrap_or_else(|e| panic!("read {TEMPLATE}: {e}"));
    let formula = render(&template, &vars);

    write(
        &PathBuf::from(var("OUT_DIR")).join(format!("{name}.rb")),
        &formula,
    );
    if let Ok(out) = env::var("BRACCO_FORMULA_OUT") {
        write(Path::new(&out), &formula);
    }
}
