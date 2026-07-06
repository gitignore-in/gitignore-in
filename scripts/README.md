# scripts/

Shell scripts for release preparation, CI validation, and repository administration.
None of these scripts run as part of the end-user CLI; they support the maintainer workflow.

## CI helpers

Called from GitHub Actions workflows. Work with the default `GITHUB_TOKEN`.

| Script | Caller workflow |
| --- | --- |
| `check-release-version.sh` | `binary-release.yml`, `cratesio-publish.yml` |
| `set-version.sh` | `prepare-release-pr.yml` |

## Admin tools (`admin/`)

Require manual execution with elevated credentials. **Not for CI.**

| Script | Required scope |
| --- | --- |
| `admin/configure-security-settings.sh` | `admin:repo` OAuth scope |

## Adding a new script

Place scripts here when they support the build/release/admin cycle for this repository.
If a script must run as part of the `gitignore.in` binary itself, it belongs in `src/` instead.
