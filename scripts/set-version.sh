#!/usr/bin/env bash
set -euo pipefail

version="${1:-}"
if [ -z "${version}" ]; then
	echo "usage: $0 <version>" >&2
	exit 1
fi

if ! [[ "${version}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
	echo "version must be in x.y.z format: ${version}" >&2
	exit 2
fi

VERSION="${version}" perl -0pi -e 's/^version = ".*"$/version = "$ENV{VERSION}"/m' Cargo.toml
VERSION="${version}" perl -0pi -e 's/(name = "gitignore-in"\nversion = )"[^"]*"/$1"$ENV{VERSION}"/m' Cargo.lock
