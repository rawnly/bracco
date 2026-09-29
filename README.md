# fff-picker

Fuzzy file picker for the terminal: git-modified files first, respects
`.gitignore`. Built on [fff-search](https://crates.io/crates/fff-search) and
[ratatui](https://ratatui.rs).

The interactive UI is drawn on `/dev/tty`; the selected path goes to stdout, so
it composes with shells and editors:

```sh
$EDITOR "$(fff-picker)"
```

Exit codes: `0` selected, `1` cancelled / no match, `2` error.

## Install

With [mise](https://mise.jdx.dev):

```sh
mise use -g github:rawnly/fff-picker
```

Or download an archive for your platform from
[Releases](https://github.com/rawnly/fff-picker/releases). Each archive contains:

```
bin/fff-picker
share/man/man1/fff-picker.1
share/usage/fff-picker.kdl
share/bash-completion/completions/fff-picker
share/zsh/site-functions/_fff-picker
share/fish/vendor_completions.d/fff-picker.fish
```

Or build from source:

```sh
cargo install --git https://github.com/rawnly/fff-picker --locked
```

### Verify a download

Every release ships a signed [packslip](https://packslip.dev) bundle
(`packslip.sigstore.json`) covering all archives, signed keylessly by the
release workflow:

```sh
packslip verify packslip.sigstore.json \
  --identity-prefix https://github.com/rawnly/fff-picker/.github/workflows/release.yml@ \
  --issuer https://token.actions.githubusercontent.com \
  --artifact fff-picker-aarch64-apple-darwin.tar.xz
```

Build provenance is also attested:
`gh attestation verify fff-picker-aarch64-apple-darwin.tar.xz --repo rawnly/fff-picker`.

## Usage

```
fff-picker [FLAGS] [DIR]

  -q, --query <QUERY>      Initial query (interactive) or the query to run (--list)
  -l, --list               Non-interactive: print matches to stdout and exit
  -n, --limit <LIMIT>      Max results for --list (default: 100)
  -a, --absolute           Print absolute paths
  -0, --print0             Separate output paths with NUL instead of newline
      --status <STATUS>    all, changed, staged, unstaged, untracked, clean
  -c, --changed            Shortcut for --status changed
  -e, --ext <EXT>          Only files with this extension (repeatable / comma-separated)
  -x, --exclude <GLOB>     Exclude paths matching this glob (repeatable)
      --vim                Vim-style modal keys (j/k, J/K, g/G, / to search) [env: FFF_PICKER_VIM]
      --log-file <FILE>    Write logs to FILE (level via RUST_LOG) [env: FFF_PICKER_LOG_FILE]
```

Run `fff-picker --help` or `man fff-picker` for the full reference.

Shell completions can also be printed directly:

```sh
fff-picker completions zsh > ~/.zfunc/_fff-picker   # bash, zsh, fish, elvish, nu, powershell
```

## Herdr plugin

`herdr-plugin.toml` adds a `pick` action that opens a popup with the picker and
then opens the chosen file in your editor (`$FFF_PICKER_EDITOR`, `$VISUAL`,
`$EDITOR`, else `vi`) in an overlay pane.

```sh
herdr plugin link .                # or: herdr plugin install rawnly/fff-picker
herdr plugin action invoke pick --plugin dev.rawnly.fff-picker
```

Requires `fff-picker` (and optionally `jq`) in `PATH`; bind the action to a key
in your herdr config.

## Development

```sh
cargo fmt --all                               # CI checks formatting
mbx clippy --all-targets -- -D warnings       # or plain `cargo`
mbx test
scripts/generate.sh                           # completions + man page into dist/ (man page needs `usage`)
```

CI (`.github/workflows/ci.yml`) runs fmt, clippy and tests on Linux and macOS
for every push to `main` and every pull request, with
[mr boxington](https://mr-boxington.jdx.dev) caching.

## Releasing

Releases are fully automated by `.github/workflows/release.yml` and triggered
by pushing a `v*` tag.

1. **Bump the version** in `Cargo.toml` and refresh the lockfile:

   ```sh
   # edit Cargo.toml: version = "0.2.0"
   cargo check            # updates Cargo.lock
   ```

2. **Commit and push to `main`**, and wait for CI to go green:

   ```sh
   git commit -am "release: v0.2.0"
   git push origin main
   ```

3. **Tag and push the tag.** The tag must be `v` + the exact `Cargo.toml`
   version, otherwise the build fails early:

   ```sh
   git tag v0.2.0
   git push origin v0.2.0
   ```

4. **Watch the workflow** (`gh run watch`). It runs four jobs:

   | Job                | What it does                                                                                                       | Permissions                                   |
   | ------------------ | ------------------------------------------------------------------------------------------------------------------ | --------------------------------------------- |
   | `build`            | Builds `x86_64`/`aarch64` for Linux (gnu) and macOS, generates completions + man page, packages `fff-picker-<target>.tar.xz` | `contents: read`                              |
   | `release`          | Creates the GitHub release (auto-generated notes) and uploads the archives                                         | `contents: write`                             |
   | `packslip`         | Downloads the archives from the release, attests them, signs the packslip manifest with the workflow's OIDC identity | `contents: read`, `id-token`, `attestations` |
   | `publish-packslip` | Uploads only `packslip.sigstore.json` to the release                                                               | `contents: write`                             |

   Signing runs in a job that cannot modify the release; a separate minimal job
   uploads the bundle. Release builds use mbx with a local-only cache, so no
   cached objects from CI can end up in published binaries.

5. **Check the published release**: download the bundle and an archive from
   the release page and run the [verify](#verify-a-download) command above.

### If something fails

- **Tag/version mismatch** — delete the tag (`git push --delete origin v0.2.0 && git tag -d v0.2.0`), fix `Cargo.toml`, and re-tag.
- **Failure after the release was created** — re-run the failed jobs from the
  Actions UI (`gh run rerun <id> --failed`). `packslip` re-downloads assets
  from the release and the bundle upload uses `--clobber`, so re-runs are safe.
- **Starting over** — delete the release and the tag, then push the tag again:
  `gh release delete v0.2.0 --cleanup-tag --yes`.
