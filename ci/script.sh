#!/bin/bash

set -ex

if [ -z "$DEBIAN_VARIANT" ]; then
    docker-compose build --build-arg UBUNTU_VERSION=${UBUNTU_VERSION} \
                         --build-arg COLLECTD_VERSION=${COLLECTD_VERSION} \
                         --build-arg RUST_TARGET=$RUST_TARGET \
                         app
    docker-compose up --abort-on-container-exit --exit-code-from app
else
    cargo build --all
    cargo install cargo-deb
    cargo deb --variant $DEBIAN_VARIANT
fi
