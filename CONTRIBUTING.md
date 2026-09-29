# Contributing

Thanks for helping out! Issues and pull requests are welcome.

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
   | `build`            | Builds `x86_64`/`aarch64` for Linux (gnu) and macOS, generates completions + man page, packages `bracco-<target>.tar.xz` | `contents: read`                              |
   | `release`          | Creates the GitHub release (auto-generated notes) and uploads the archives                                         | `contents: write`                             |
   | `packslip`         | Downloads the archives from the release, attests them, signs the packslip manifest with the workflow's OIDC identity | `contents: read`, `id-token`, `attestations` |
   | `publish-packslip` | Uploads only `packslip.sigstore.json` to the release                                                               | `contents: write`                             |

   Signing runs in a job that cannot modify the release; a separate minimal job
   uploads the bundle. Release builds use mbx with a local-only cache, so no
   cached objects from CI can end up in published binaries.

5. **Check the published release**: download the bundle and an archive from
   the release page and run the [verify](#verify-a-download) command above.

### Homebrew formula

`build.rs` renders `packaging/homebrew/formula.rb.in` using the metadata in
`Cargo.toml` (version, description, license, repository), so edit those, not the
template output. The `homebrew` release job downloads the published archives,
exports their checksums as `BRACCO_SHA256_<TARGET>` and runs
`BRACCO_FORMULA_OUT=Formula/bracco.rb cargo check` to render the final formula,
then pushes it to [`rawnly/homebrew-tap`](https://github.com/rawnly/homebrew-tap).

One-time setup: create an SSH deploy key with write access on the tap repo and
store its private half as the `HOMEBREW_TAP_DEPLOY_KEY` repository secret.
Without the secret the job still renders the formula and uploads it as the
`homebrew-formula` artifact, but skips the push.

```sh
ssh-keygen -t ed25519 -N "" -C "bracco release" -f /tmp/bracco-tap-key
gh repo deploy-key add /tmp/bracco-tap-key.pub --repo rawnly/homebrew-tap --allow-write --title "bracco release"
gh secret set HOMEBREW_TAP_DEPLOY_KEY --repo rawnly/bracco < /tmp/bracco-tap-key
rm -P /tmp/bracco-tap-key /tmp/bracco-tap-key.pub
```

Render locally (checksums default to zeros): `BRACCO_FORMULA_OUT=/tmp/bracco.rb cargo check`.

### crates.io

The `crates-io` release job runs `cargo publish --locked` after the GitHub
release succeeds. One-time setup: add a `CARGO_REGISTRY_TOKEN` repository secret
(a crates.io API token with the `publish-new` and `publish-update` scopes).
Without it the job is skipped. Publishing is irreversible (a version can only be
yanked), so check `cargo publish --dry-run` when changing package metadata.

### If something fails

- **Tag/version mismatch** — delete the tag (`git push --delete origin v0.2.0 && git tag -d v0.2.0`), fix `Cargo.toml`, and re-tag.
- **Failure after the release was created** — re-run the failed jobs from the
  Actions UI (`gh run rerun <id> --failed`). `packslip` re-downloads assets
  from the release and the bundle upload uses `--clobber`, so re-runs are safe.
- **Starting over** — delete the release and the tag, then push the tag again:
  `gh release delete v0.2.0 --cleanup-tag --yes`.
