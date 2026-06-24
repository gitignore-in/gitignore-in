#!/bin/sh
# Check that Dependabot vulnerability alerts and security updates are enabled.
#
# Usage: ./scripts/check-dependabot-security-updates.sh
#
# Exit 1 if vulnerability alerts are disabled or Dependabot security updates
# are disabled. Exit 0 if both are enabled.
#
# Environment variables:
#   GH_TOKEN             - GitHub token (required; set by actions/github-token or
#                          secrets.GITHUB_TOKEN in GitHub Actions)
#   GITHUB_REPOSITORY    - owner/repo (default: gitignore-in/gitignore-in)

set -eu

REPO="${GITHUB_REPOSITORY:-gitignore-in/gitignore-in}"

gh api "repos/${REPO}/vulnerability-alerts" \
	--silent --fail ||
	{
		printf '::error::Vulnerability alerts are disabled for %s.\n' "${REPO}"
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
