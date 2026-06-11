# Contributing

## Development

Install a stable Rust toolchain. All CI checks can be reproduced locally:

```sh
cargo fmt --all -- --check   # formatting
cargo clippy -- -D warnings  # lint
cargo test                   # tests
cargo build                  # build
```

Shell scripts are formatted with `shfmt`:

```sh
go run mvdan.cc/sh/v3/cmd/shfmt@v3.13.1 -d scripts/*.sh
```

## Releasing

Releases are cut from `main`. The release workflow is:

1. **Prepare the release PR** (via workflow dispatch on `prepare release PR`):
   - Runs `scripts/set-version.sh <version>` which updates both `Cargo.toml` and `Cargo.lock`.
   - Opens a PR from `release/v<version>` into `main`.
2. **Review and merge the release PR**: CI runs `cargo publish --dry-run` against the release branch.
3. **Create a GitHub release** tagged `v<version>`: this triggers `cargo publish` (production) and the binary release workflow.

### Why both Cargo.toml and Cargo.lock must be updated

`scripts/set-version.sh` updates both files in one step. Updating only `Cargo.toml`
leaves `Cargo.lock` with the old `gitignore-in` version; the subsequent `cargo build`
in CI regenerates `Cargo.lock` locally but does not commit it, so `main` retains a
stale lock after merge. The perl substitution in `set-version.sh` keeps both files
in sync as part of the release PR diff.

## Dependency updates

Renovate manages most dependency updates automatically. For dependency PRs that fail
CI, check whether the `Cargo.lock` has diverged from `Cargo.toml`.
