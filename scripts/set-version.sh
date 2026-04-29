#!/usr/bin/env bash
set -euo pipefail

version="${1:-}"
if [ -z "${version}" ]; then
	echo "usage: $0 <version>" >&2
	exit 1
fi

perl -0pi -e 's/^version = ".*"$/version = "'"${version}"'"/m' Cargo.toml
