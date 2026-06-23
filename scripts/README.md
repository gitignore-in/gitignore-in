# scripts/

Shell scripts for release preparation, CI validation, and repository administration.
None of these scripts run as part of the end-user CLI; they support the maintainer workflow.

| Script | When it runs | Purpose |
| --- | --- | --- |
| `set-version.sh` | Release prep (manual / `prepare-release-pr.yml`) | Updates the version string in `Cargo.toml` and `Cargo.lock` |
| `check-release-version.sh` | CI (`binary-release.yml`, `cratesio-publish.yml`) | Validates that the release tag matches the version in `Cargo.toml` |
| `configure-security-settings.sh` | One-off admin task | Enables GitHub Secret Scanning and Push Protection (requires admin token) |

## Adding a new script

Place scripts here when they support the build/release/admin cycle for this repository.
If a script must run as part of the `gitignore.in` binary itself, it belongs in `src/` instead.
