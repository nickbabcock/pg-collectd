#!/bin/bash

set -euo pipefail

source $HOME/.cargo/env

cargo build

cp target/debug/libpg_collectd.so /usr/lib/collectd/pg_collectd.so

cat <<EOF | tee /etc/collectd/collectd.conf
Hostname "localhost"
LoadPlugin load
LoadPlugin pg_collectd
Interval 1

<Plugin pg_collectd>
	Connection "postgresql://collectd:hellocollectd@timescale:5432/timescale_built"
	BatchSize 5
</Plugin>
EOF

service collectd start
sleep 15
service collectd status

COUNT=$(PGPASSWORD=my_rust_test psql -t -h timescale timescale_built postgres -c "SELECT COUNT(*) from collectd_metrics")

if [ "$COUNT" -gt  "0" ]; then
	exit 0
fi

echo "$COUNT"
exit 1
