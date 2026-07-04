#!/bin/sh
# Enable GitHub Secret Scanning and Push Protection for this repository.
#
# Usage: ./scripts/admin/configure-security-settings.sh [--dry-run]
#
# Requires: gh CLI authenticated with a token that has admin:repo scope.
# Idempotent: exits 0 if settings are already in the desired state.
#
# Settings applied:
#   - secret_scanning: enabled
#   - secret_scanning_push_protection: enabled

set -eu

REPO="gitignore-in/gitignore-in"
DRY_RUN=false

if [ "${1:-}" = "--dry-run" ]; then
	DRY_RUN=true
fi

current=$(gh api "repos/${REPO}" \
	--jq '{
    ss: .security_and_analysis.secret_scanning.status,
    pp: .security_and_analysis.secret_scanning_push_protection.status
  }')

ss_status=$(printf '%s' "${current}" | jq -r '.ss')
pp_status=$(printf '%s' "${current}" | jq -r '.pp')

if [ "${ss_status}" = "enabled" ] && [ "${pp_status}" = "enabled" ]; then
	echo "Secret Scanning and Push Protection are already enabled"
	exit 0
fi

if "${DRY_RUN}"; then
	echo "Dry run: would enable secret_scanning=${ss_status} push_protection=${pp_status}"
	exit 0
fi

gh api -X PATCH "repos/${REPO}" \
	--field "security_and_analysis[secret_scanning][status]=enabled" \
	--field "security_and_analysis[secret_scanning_push_protection][status]=enabled" \
	>/dev/null

echo "Enabled Secret Scanning and Push Protection on ${REPO}"
