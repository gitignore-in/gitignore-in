# scripts/

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
