#!/bin/sh
#
# Publishes all crates to crates.io and tags the version in git.
# Also pushes the tags.
#

set -e

echo "Checking crate versions"
./check_version.sh

VERSION=$(grep "^version = " remoc/Cargo.toml | cut -d ' ' -f 3 | tr -d \")

echo "Publishing remoc_macro $VERSION"
cargo publish --manifest-path remoc_macro/Cargo.toml

# Required for crates.io index update.
sleep 20

echo "Publishing remoc $VERSION"
cargo publish --manifest-path remoc/Cargo.toml

echo "Tagging version in git"
git tag "v$VERSION"
git push
git push --tags
