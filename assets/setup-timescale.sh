#!/bin/bash
set -e

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null && pwd )"

SCHEMA=$(cat "$DIR/../sql/schema.sql")
POLICIES=$(cat "$DIR/../sql/policies.sql")

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<EOF
    CREATE DATABASE timescale_built;
    CREATE DATABASE timescale_built2;
EOF

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "timescale_built" <<EOF
    CREATE USER collectd WITH PASSWORD 'hellocollectd';
    $SCHEMA
    $POLICIES
    GRANT INSERT ON collectd_metrics TO collectd;
EOF

# The insert_values unit test uses this database. Its rows are from 2004, so
# this database has no policies. The retention policy can delete the rows
# while the test runs.
psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "timescale_built2" <<EOF
    $SCHEMA
    GRANT INSERT ON collectd_metrics TO collectd;
EOF
