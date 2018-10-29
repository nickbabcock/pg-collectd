#!/bin/bash

set -e

cargo release "$@"

COLLECTD_VERSION="5.7" cargo deb --variant "collectd57"
COLLECTD_VERSION="5.5" cargo deb --variant "collectd55"
COLLECTD_VERSION="5.4" cargo deb --variant "collectd54"
