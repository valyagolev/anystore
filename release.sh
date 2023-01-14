#!/bin/bash -ex

VERSION=$(cargo metadata --format-version 1 | jq ".packages | map(select( .name == \"anystore\" ))[0].version" -r)

git tag -a v$VERSION -m "Release $VERSION"
git push origin v$VERSION

cargo publish --dry-run
cargo publish