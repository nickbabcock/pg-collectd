#!/bin/bash

set -euo pipefail

source $HOME/.cargo/env

cargo build
cargo test

cp target/debug/libpg_collectd.so /usr/lib/collectd/pg_collectd.so

cat <<EOF | tee /etc/collectd/collectd.conf
Hostname "localhost"
Interval 1
LoadPlugin logfile
LoadPlugin load
LoadPlugin pg_collectd

<Plugin logfile>
	LogLevel info
	File "/var/log/collectd.log"
</Plugin>

<Plugin pg_collectd>
	Connection "postgresql://collectd:hellocollectd@timescale:5432/timescale_built"
	BatchSize 5
	LogTimings INFO
</Plugin>
EOF

# Run collectd directly. The init script uses collectdmon, which can hang in
# containers with a large open file limit.
collectd -f -C /etc/collectd/collectd.conf &
COLLECTD_PID=$!
sleep 15

if ! kill -0 "$COLLECTD_PID"; then
	echo "collectd is not running"
	cat /var/log/collectd.log || true
	exit 1
fi

kill "$COLLECTD_PID"
wait "$COLLECTD_PID" || true

COUNT=$(PGPASSWORD=my_rust_test psql -tA -h timescale timescale_built postgres -c "SELECT COUNT(*) from collectd_metrics")

echo "rows: $COUNT"
cat /var/log/collectd.log

if [ "$COUNT" -le "0" ]; then
	echo "no rows were inserted"
	exit 1
fi

if ! grep -q 'pg_collectd: .*inserted [0-9]* rows' /var/log/collectd.log; then
	echo "pg_collectd did not log its insert timings"
	exit 1
fi
