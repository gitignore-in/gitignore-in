#!/usr/bin/env bash
set -euo pipefail

release_ref="${1:-}"
if [ -z "${release_ref}" ]; then
	echo "usage: $0 <release-tag-or-release-branch>" >&2
	exit 2
fi

case "${release_ref}" in
refs/tags/v* | v* | release/v*)
	release_version="${release_ref##*/}"
	release_version="${release_version#v}"
	;;
*)
	echo "release ref must be v<version> or release/v<version>: ${release_ref}" >&2
	exit 1
	;;
esac

cargo_version="$(
	perl -ne 'if (/^version = "([^"]+)"$/) { print "$1\n"; exit }' Cargo.toml
)"

if [ -z "${cargo_version}" ]; then
	echo "failed to read package version from Cargo.toml" >&2
	exit 1
fi

if [ "${release_version}" != "${cargo_version}" ]; then
	echo "release version ${release_version} does not match Cargo.toml version ${cargo_version}" >&2
	exit 1
fi

echo "release version matches Cargo.toml: ${cargo_version}"
