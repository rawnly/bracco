#!/usr/bin/env bash
# Generate shell completions and the man page into dist/.
#   scripts/generate.sh [path-to-bracco]
# Completions come from the binary itself (usage-rs); the man page is rendered
# from the binary's usage spec by the `usage` CLI (https://usage.jdx.dev).
set -euo pipefail

cd "$(dirname "$0")/.."
bin="${1:-}"
if [ -z "$bin" ]; then
  cargo build --release --quiet
  bin=target/release/bracco
fi

out=dist
mkdir -p "$out/completions" "$out/man"

"$bin" completions bash       > "$out/completions/bracco.bash"
"$bin" completions zsh        > "$out/completions/_bracco"
"$bin" completions fish       > "$out/completions/bracco.fish"
"$bin" completions elvish     > "$out/completions/bracco.elv"
"$bin" completions nu         > "$out/completions/bracco.nu"
"$bin" completions powershell > "$out/completions/bracco.ps1"
"$bin" spec       > "$out/bracco.usage.kdl"

if command -v usage >/dev/null; then
  usage generate manpage -f "$out/bracco.usage.kdl" -o "$out/man/bracco.1"
else
  echo "warning: \`usage\` CLI not found, skipping man page (mise use -g usage)" >&2
fi

echo "generated in $out/:"; find "$out" -type f | sort
