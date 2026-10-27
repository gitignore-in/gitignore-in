#!/bin/sh
# Check that Dependabot vulnerability alerts and security updates are enabled.
#
# Usage: ./scripts/check-dependabot-security-updates.sh
#
# Exit 1 if vulnerability alerts are disabled or Dependabot security updates
# are disabled. Exit 0 if both are enabled.
#
# Environment variables:
#   GH_TOKEN             - Fine-grained GitHub token with repository
#                          Administration: read permission (required)
#   GITHUB_REPOSITORY    - owner/repo (default: gitignore-in/gitignore-in)

set -eu

REPO="${GITHUB_REPOSITORY:-gitignore-in/gitignore-in}"

if [ -z "${GH_TOKEN:-}" ]; then
	printf '::error::Set DEPENDABOT_STATUS_TOKEN to a token with repository Administration: read permission.\n'
	exit 1
fi

gh api "repos/${REPO}/vulnerability-alerts" \
	--silent ||
	{
		printf '::error::Could not verify vulnerability alerts for %s; check the token permissions and repository settings.\n' "${REPO}"
		printf '::error::Enable at: https://github.com/%s/settings/security_analysis\n' "${REPO}"
		exit 1
	}
echo "Vulnerability alerts are enabled."

status=$(gh api "repos/${REPO}" \
	--jq '.security_and_analysis.dependabot_security_updates.status')
if [ "${status}" != "enabled" ]; then
	printf '::error::Dependabot security updates are disabled (status: %s).\n' "${status}"
	printf '::error::Enable at: https://github.com/%s/settings/security_analysis\n' "${REPO}"
	exit 1
fi
echo "Dependabot security updates are enabled."
