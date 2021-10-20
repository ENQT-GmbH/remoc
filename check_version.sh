#!/bin/sh
#
# Checks that the remoc and remoc_macro crates have the same version and
# remoc depends on the exact same remoc_macro version.
#

VERSION=$(grep "^version = " remoc/Cargo.toml | cut -d ' ' -f 3 | tr -d \")
MACRO_VERSION=$(grep "^version = " remoc_macro/Cargo.toml | cut -d ' ' -f 3 | tr -d \")
MACRO_DEP=$(grep 'remoc_macro = ' remoc/Cargo.toml | cut -d ' ' -f 6 | tr -d \",=)

if [ "$VERSION" != "$MACRO_VERSION" ] || [ "$VERSION" != "$MACRO_DEP" ] ; then
    echo "Version mismatch!"
    echo "remoc                  version is $VERSION"
    echo "remoc_macro            version is $MACRO_VERSION"
    echo "remoc_macro dependency version is $MACRO_DEP"
    exit 1
fi

exit 0
