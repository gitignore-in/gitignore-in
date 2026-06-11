#!/usr/bin/env bash
set -euo pipefail

version="${1:-}"
if [ -z "${version}" ]; then
	echo "usage: $0 <version>" >&2
	exit 1
fi

perl -0pi -e 's/^version = ".*"$/version = "'"${version}"'"/m' Cargo.toml
perl -0pi -e 's/(name = "gitignore-in"\nversion = )"[^"]*"/$1"'"${version}"'"/m' Cargo.lock
