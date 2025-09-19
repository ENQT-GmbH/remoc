#!/bin/sh
#
# Publishes all crates to crates.io and tags the version in git.
# Also pushes the tags.
#

set -e

echo "Checking crate versions"
./check_version.sh

VERSION=$(grep "^version = " Cargo.toml | cut -d ' ' -f 3 | tr -d \")

echo "Publishing $VERSION"
cargo publish --workspace

echo "Tagging version in git"
git tag "v$VERSION"
git push
git push --tags
