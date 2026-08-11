#!/usr/bin/env bash

set -euo pipefail

if [ "$#" -lt 3 ]; then
  echo "usage: $0 <artifact-dir> <tag> <arch> [<arch> ...]" >&2
  exit 2
fi

artifact_dir="$1"
tag="$2"
shift 2

if [ ! -d "${artifact_dir}" ]; then
  echo "artifact directory not found: ${artifact_dir}" >&2
  exit 1
fi

expected_file="$(mktemp)"
actual_file="$(mktemp)"
missing_file="$(mktemp)"
unexpected_file="$(mktemp)"
trap 'rm -f "${expected_file}" "${actual_file}" "${missing_file}" "${unexpected_file}"' EXIT

for arch in "$@"; do
  printf 'gitignore-in-%s-%s.tar.gz\n' "${arch}" "${tag}"
done | sort > "${expected_file}"

find "${artifact_dir}" -maxdepth 1 -type f -name '*.tar.gz' -exec basename {} \; | sort > "${actual_file}"

comm -23 "${expected_file}" "${actual_file}" > "${missing_file}"
comm -13 "${expected_file}" "${actual_file}" > "${unexpected_file}"

if [ -s "${missing_file}" ] || [ -s "${unexpected_file}" ]; then
  echo "release artifact set mismatch for tag ${tag}" >&2
  if [ -s "${missing_file}" ]; then
    echo "missing artifacts:" >&2
    sed 's/^/  - /' "${missing_file}" >&2
  fi
  if [ -s "${unexpected_file}" ]; then
    echo "unexpected artifacts:" >&2
    sed 's/^/  - /' "${unexpected_file}" >&2
  fi
  exit 1
fi

echo "verified release artifacts for tag ${tag}" >&2
