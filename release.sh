#!/bin/bash

set -e

echo "prev: $PREV_VERSION, new: $NEW_VERSION, dry run $DRY_RUN"

# We temporarily switch out the verion of cargo with the new version so that we
# can build our debian packages with the proper version
sed -i -e "s/version = \"$PREV_VERSION\"/version = \"$NEW_VERSION\"/" Cargo.toml

mkdir -p ./bin
COLLECTD_VERSION="5.7" cargo deb --variant "collectd57"
mv ./target/debian/* ./bin
COLLECTD_VERSION="5.5" cargo deb --variant "collectd55"
mv ./target/debian/* ./bin
COLLECTD_VERSION="5.4" cargo deb --variant "collectd54"
mv ./target/debian/* ./bin

sed -i -e "s/version = \"$NEW_VERSION\"/version = \"$PREV_VERSION\"/" Cargo.toml

# Refresh build so that Cargo.lock doesn't change its reference to $NEW_VERSION
cargo build
