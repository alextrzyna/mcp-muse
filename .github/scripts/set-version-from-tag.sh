#!/usr/bin/env bash
# Stamp Cargo.toml's package version from a release tag (v2026.9.9 -> 2026.9.9).
#
# The auto-tag workflow cannot push its version bump to the protected main
# branch, so the checked-in version lags behind the tags. The release
# workflow runs this before building and publishing so binaries and the
# crates.io upload carry the tag's version.
set -euo pipefail

tag="${1:?usage: set-version-from-tag.sh vYYYY.M.PATCH}"
version="${tag#v}"
if ! [[ "$version" =~ ^[0-9]{4}\.[0-9]{1,2}\.[0-9]+$ ]]; then
  echo "Tag '$tag' is not a CalVer release tag (vYYYY.M.PATCH)" >&2
  exit 1
fi

# BSD and GNU sed differ on -i; write to a temp file instead.
sed "s/^version = \".*\"/version = \"$version\"/" Cargo.toml > Cargo.toml.new
mv Cargo.toml.new Cargo.toml
grep -q "^version = \"$version\"" Cargo.toml
echo "Cargo.toml version set to $version"
