#!/usr/bin/env bash
# Generate shell completions and the man page into dist/.
#   scripts/generate.sh [path-to-fff-picker]
# Completions come from the binary itself (usage-rs); the man page is rendered
# from the binary's usage spec by the `usage` CLI (https://usage.jdx.dev).
set -euo pipefail

cd "$(dirname "$0")/.."
bin="${1:-}"
if [ -z "$bin" ]; then
  cargo build --release --quiet
  bin=target/release/fff-picker
fi

out=dist
mkdir -p "$out/completions" "$out/man"

"$bin" completions bash       > "$out/completions/fff-picker.bash"
"$bin" completions zsh        > "$out/completions/_fff-picker"
"$bin" completions fish       > "$out/completions/fff-picker.fish"
"$bin" completions elvish     > "$out/completions/fff-picker.elv"
"$bin" completions nu         > "$out/completions/fff-picker.nu"
"$bin" completions powershell > "$out/completions/fff-picker.ps1"
"$bin" spec       > "$out/fff-picker.usage.kdl"

if command -v usage >/dev/null; then
  usage generate manpage -f "$out/fff-picker.usage.kdl" -o "$out/man/fff-picker.1"
else
  echo "warning: \`usage\` CLI not found, skipping man page (mise use -g usage)" >&2
fi

echo "generated in $out/:"; find "$out" -type f | sort
