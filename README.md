<p align="center">
  <img src="assets/bracco-logo.png" alt="bracco" width="220">
</p>

<h1 align="center">bracco</h1>

<p align="center">
  A fast fuzzy file picker for the terminal: git-modified files first, <code>.gitignore</code> respected.<br>
  <em>bracco</em> (BRAHK-koh) is the Italian pointer dog: it sniffs out what you changed and points at it.
</p>

<p align="center">
  <a href="https://github.com/rawnly/bracco/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/rawnly/bracco/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/rawnly/bracco/releases"><img alt="Release" src="https://img.shields.io/github/v/release/rawnly/bracco"></a>
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
</p>

## Features

- **Git-aware ranking**: modified, staged and untracked files float to the top; filter by status with `tab`.
- **Fast**: built on [fff-search](https://crates.io/crates/fff-search), respects `.gitignore`.
- **Preview pane**: show any command's output (`bat`, `git diff`, ...) next to the list.
- **Vim mode**: `--vim` for modal `j`/`k`/`/` navigation.
- **Composable**: UI on `/dev/tty`, result on stdout. Works with `$EDITOR "$(bracco)"`, pipes and `--list`.
- **[Herdr](https://herdr.dev) and [Rex](https://www.superlogical.com/rex) plugins** included.

<p align="center">
  <img src="assets/bracco-showcase.gif" alt="bracco-showcase" >
</p>

## Install

With [Homebrew](https://brew.sh):

```sh
brew install rawnly/tap/bracco
```

With [mise](https://mise.jdx.dev):

```sh
mise use -g github:rawnly/bracco
```

From [crates.io](https://crates.io/crates/bracco) (needs a [Rust toolchain](https://www.rust-lang.org/tools/install)):

```sh
cargo install bracco --locked
# or the latest main:
cargo install --git https://github.com/rawnly/bracco --locked
```

Or download an archive for your platform from
[Releases](https://github.com/rawnly/bracco/releases). It contains the binary,
man page, usage spec and shell completions.

<details>
<summary>Verify a download</summary>

Every release ships a signed [packslip](https://packslip.dev) bundle
(`packslip.sigstore.json`) covering all archives, signed keylessly by the
release workflow:

```sh
packslip verify packslip.sigstore.json \
  --identity-prefix https://github.com/rawnly/bracco/.github/workflows/release.yml@ \
  --issuer https://token.actions.githubusercontent.com \
  --artifact bracco-aarch64-apple-darwin.tar.xz
```

Build provenance is also attested:
`gh attestation verify bracco-aarch64-apple-darwin.tar.xz --repo rawnly/bracco`.

</details>

## Quick start

```sh
bracco                              # pick a file, print its path
$EDITOR "$(bracco)"                 # open it in your editor
bracco --edit --keep-open           # open in $VISUAL/$EDITOR and come back
bracco -c --preview 'git diff --color=always {}'   # changed files with a diff preview
bracco --vim                        # modal keys
bracco -l -q '*.rs' -n 20           # non-interactive: list matches
```

Press `?` (or `F1`) inside the picker for the full key reference.

## Usage

```
bracco [FLAGS] [DIR]

  -q, --query <QUERY>          Initial query (interactive) or the query to run (--list)
  -l, --list                   Non-interactive: print matches to stdout and exit
  -n, --limit <LIMIT>          Max results for --list (default: 100)
  -a, --absolute               Print absolute paths
  -0, --print0                 Separate output paths with NUL instead of newline
      --status <STATUS>        all, changed, staged, unstaged, untracked, clean
  -c, --changed                Shortcut for --status changed
  -e, --ext <EXT>              Only files with this extension (repeatable / comma-separated)
  -x, --exclude <GLOB>         Exclude paths matching this glob (repeatable)
  -E, --exec <CMD>             Run CMD on enter instead of printing the path ({} = path)
      --edit                   Shortcut for --exec '$VISUAL / $EDITOR / vi'
      --keep-open              With --exec / --edit: return to the picker afterwards
      --preview <CMD>          Show CMD's output for the highlighted file
      --preview-window <SPEC>  [right|left|up|down][:N%][:hidden] (default: right:50%)
      --vim                    Vim-style modal keys
      --log-file <FILE>        Write logs to FILE (level via RUST_LOG)
```

Run `bracco --help` or `man bracco` for the full reference.

**Exit codes:** `0` selected, `1` cancelled / no match, `2` error. With `--exec`,
the command's own exit code.

### Keys

| Key | Action |
| --- | --- |
| `↑` `↓` `^p` `^n` `^k` `^j` | Move selection |
| `enter` | Select (print path or run `--exec`) |
| `tab` / `shift-tab` | Cycle git status filter |
| `^g` `^s` `^a` `^t` | Toggle changed / staged / unstaged / untracked |
| `^o` | Show / hide preview |
| `^d` `^u` | Scroll preview half a page |
| `^w` `^x` | Delete last word / clear query |
| `esc` `^c` | Cancel |

**Vim mode (`--vim`)** starts in normal mode:

| Key | Action |
| --- | --- |
| `j` `k` | Move selection |
| `g` `G` | Top / bottom |
| `d` `u` | Jump 10 rows |
| `J` `K` | Scroll preview by line (`f` `b` by page) |
| `/` `i` `a` | Start searching (insert mode) |
| `esc` | Insert → normal; normal → cancel |
| `q` | Cancel |

### Query syntax

| Query | Matches |
| --- | --- |
| `text` | Fuzzy match on the path |
| `*.rs` | Only this extension |
| `/src/` | Only inside this directory |
| `!test` | Exclude matches |
| `type:rust` | By file type |
| `status:modified` | Git status (`staged`, `untracked`, ...) |

### Environment

| Variable | Equivalent flag |
| --- | --- |
| `BRACCO_EXEC` | `--exec` |
| `BRACCO_PREVIEW` | `--preview` |
| `BRACCO_VIM` | `--vim` |
| `BRACCO_LOG_FILE` | `--log-file` |

### Shell integration

Source Bracco's shell integration to enable fzf-style fuzzy path completion.
Type `nvim **` and press Tab to choose a file; the selected path is inserted
into the command line. Supported shells: bash, zsh, fish, and PowerShell. In
bash, ordinary Tab falls back to basic file completion; the other integrations
preserve their shell's normal Tab completion.

```sh
# Bash (~/.bashrc)
bracco shell-integration bash >> ~/.bashrc
# Zsh (~/.zshrc)
bracco shell-integration zsh >> ~/.zshrc
# Fish (~/.config/fish/config.fish)
bracco shell-integration fish >> ~/.config/fish/config.fish
# PowerShell ($PROFILE)
bracco shell-integration powershell >> $PROFILE
```

Elvish and Nushell currently have argument completions, but no fuzzy shell
integration. Shell argument completion is available separately:

```sh
bracco completions zsh > ~/.zfunc/_bracco   # bash, zsh, fish, elvish, nu, powershell
```

## Herdr plugin

`herdr-plugin.toml` adds a `pick` action that opens a popup with bracco and
then opens the chosen file in your editor (`$BRACCO_EDITOR`, `$VISUAL`,
`$EDITOR`, else `vi`) in an overlay pane.

```sh
herdr plugin link .                # or: herdr plugin install rawnly/bracco
herdr plugin action invoke pick --plugin dev.rawnly.bracco
```

Requires `bracco` (and optionally `jq`) in `PATH`; bind the action to a key in
your herdr config.

```toml
[[keys.command]]
key = "prefix+i"
type = "plugin_action"
command = "dev.rawnly.bracco.pick"
description = "Pick file"
```

## Rex plugin

`rex/lua/bracco` adds a `bracco_pick` action to [Rex](https://www.superlogical.com/rex).
It opens bracco in a floating pane in the focused pane's directory. The chosen
file opens in your editor (`editor` option, else `$BRACCO_EDITOR`, `$VISUAL`,
`$EDITOR`, `vi`) in a new tab, and the float closes. Cancelling just closes it.

Requires `bracco` in `PATH` on the machine running the Rex server, and `jq` to
switch to the new tab (without it the tab opens but your view stays put).

Options (all optional):

| Option | Default | Meaning |
| --- | --- | --- |
| `bin` | `"bracco"` | bracco executable |
| `editor` | env, see above | Editor command; may carry args (`"code -w"`) |
| `args` | `{}` | Extra bracco flags, e.g. `{ "--vim", "--preview", "bat {}" }` |
| `bounds` | `{ 0.2, 0.15, 0.6, 0.7 }` | Float geometry `{ x, y, w, h }` as window fractions |
| `key` | none | Bind the action, e.g. `"ctrl+a>i"` |

### With Amber

[Amber](https://github.com/rawnly/amber) installs the plugin from GitHub and
pins the commit in `amber-lock.json`. In `init.lua`:

```lua
local amber = dofile(os.getenv("HOME") .. "/.local/share/rex/amber/amber.lua")

amber.setup({
  plugins = {
    { "rawnly/bracco", main = "bracco", opts = { key = "ctrl+a>i" } },
  },
})
```

Then install and reload (config load never installs anything):

```sh
rex config reload
rex do amber_install
rex config reload
```

- Updates: `rex do amber_update plugin=rawnly/bracco`, or pin with
  `version = "^0.2.0"`.
- Trade-offs: needs Amber, `git` and network on the server. Amber clones the
  whole bracco repo (Rust sources included) just for one Lua file, and it is
  alpha and GitHub-only.
- Hacking on it: use `path = "/abs/path/to/bracco"` instead of fetching.

### Without Amber

The plugin is one dependency-free file, so copy it into your config:

```sh
mkdir -p ~/.config/rex/plugins
curl -fsSL https://raw.githubusercontent.com/rawnly/bracco/main/rex/lua/bracco/init.lua \
  -o ~/.config/rex/plugins/bracco.lua
```

Load it from `init.lua`:

```lua
local bracco = dofile(os.getenv("HOME") .. "/.config/rex/plugins/bracco.lua")
bracco.setup({ key = "ctrl+a>i" })
```

Or skip the plugin and wire it by hand: an action that opens a floating pane
running a small script, plus a binding.

```lua
rex.action({
  name = "pick_file",
  title = "Pick file",
  run = function(ctx)
    return rex.session.new_layer({
      bounds = { 0.2, 0.15, 0.6, 0.7 },
      focus = true,
      layout = rex.layout.block({
        flavor = "com.superlogical.terminal.shell",
        options = { command = { "sh", os.getenv("HOME") .. "/.config/rex/bin/pick-file.sh" } },
      }),
    })
  end,
})
rex.bind("ctrl+a>i", "pick_file")
```

```sh
#!/bin/sh
# pick-file.sh: pick with bracco, open the result in a new, focused tab.
f=$(bracco --absolute) || exit 0
[ -n "$f" ] || exit 0
win=$(rex window new -c "$PWD" --focus=false --json "$(basename "$f")" -- nvim "$f" | jq -r .window_id) || exit 1
rex do session.select session_id="$REX_SESSION" window_id="$win"
exec rex block close "$REX_BLOCK"
```

- Trade-offs: no installer, no lockfile, no updates: re-download to upgrade.
  In exchange nothing else runs on the server, and a hand-written script is
  easy to bend (different editor per file type, send to a split instead of a
  tab).

| | Amber | Copy the file | By hand |
| --- | --- | --- | --- |
| Setup | `amber.setup` + install | `curl` + `dofile` | write action + script |
| Updates | `amber_update`, locked | re-download | you own it |
| Extra dependencies | Amber, git | none | none |
| Customizable | options | options or edit file | fully |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and the release process.

## License

[MIT](LICENSE) © Federico Vitale
