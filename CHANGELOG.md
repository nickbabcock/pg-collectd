# 0.3.0 - 2026-11-12

A maintenance release that updates internal dependencies. It also puts the `type` and `type_instance` values into the correct columns.

**Breaking changes:**

- pg-collectd now names the columns when it copies rows to postgres. Thus, the table must use the column names from the README.
- The `type` and `type_instance` values now go into the correct columns. Older releases swapped the two values if the table had the column order from the README (`type_instance` before `type`).

To correct the old rows in a table with the README column order, run this statement one time. Use the time when you upgraded pg-collectd:

```sql
UPDATE collectd_metrics
SET type = type_instance, type_instance = type
WHERE time < '<time of upgrade>';
```

The statement uses the time of the metric, not the time that pg-collectd wrote the row. Thus, examine the rows near the upgrade time. For example, after a restart collectd can send old values. Do not run the statement two times, because each run swaps the values again. If more than one pg-collectd writes to the table, and you upgraded them at different times, run the statement for each pg-collectd. Add a condition on `host` for the hosts that each pg-collectd writes.

If you use TimescaleDB compression, you must have TimescaleDB 2.11 or later to update compressed chunks. Also update the queries and dashboards that use the swapped columns.

Other changes:

- Update collectd-plugin from 0.12 to 0.16.1
- Update the postgres driver to 0.19
- Release one deb package, `pg-collectd_<version>-1_amd64.deb`, for collectd 5.7 to 5.12. It replaces the separate deb packages for each collectd version. The deb package now declares its libc dependency (glibc 2.34 or later, for example Ubuntu 22.04 or Debian 12).
- Remove support for collectd 5.4, 5.5, and 5.6
- Fix: when postgres rejected a batch on a new connection, pg-collectd kept the batch and sent it again with the next batch. Now pg-collectd discards the rejected batch, as it already did on an existing connection.
- When pg-collectd sends a batch, it connects a maximum of one time each second, as before. Now it keeps new data in memory until the next connect attempt. Before, it discarded that data.
- A flush with no new data no longer sends an empty insert to postgres
- To build from source, you must have Rust 1.88 or later

# 0.2.0 - 2020-04-26

Another maintenance release but contains updates to bigger internal dependencies. If compiling pg-collectd from source and using the collectd version autodetection, pg-collectd now supports collectd 5.9 - 5.11.

# 0.1.5 - 2019-10-10

* Maintenance release that bumped internal dependencies

# 0.1.4 - 2019-02-06

- Bump collectd-plugin from 0.9.0 to 0.9.1
  - Compile on non-x86 platforms
  - Add `COLLECTD_PATH` environment variable for detecting collectd version from collectd's source directory (most useful with the `bindgen` feature).
  - Output panic info into collectd logs

# 0.1.3 - 2019-01-16

Extremely minor release -- basically ensuring that the automated CI deployments worked

Internal dependency updates:
 - Update csv from 1.0.2 to 1.0.5
 - Update serde / serde_derive from 1.0.82 to 1.0.84
 - Update parking_lot from 0.7.0 to 0.7.1
 - Update failure from 0.1.3 to 0.1.5

# 0.1.2 - 2018-12-16

* Reduce memory allocations necessary after a failure inserting into the database
* If there was a db failure and we're trying to insert again within the same second -- give the db a break and discard those values
* Bumping the internal collectd plugin to 0.9 from 0.8.4 improves resiliancy that unexpected panics won't bring collectd down

# 0.1.1 - 2018-11-26

* Update to collectd-plugin 0.8.4 from 0.8.1, which fixes segfaults on plugin flush and shutdown
* Add: configurable log level for timing data using the LogTimings configuration option (an example of a log timing: "inserted 1000 rows from 1000 values from 86403 bytes (capacity: 157696) in 54ms")
* Fix batch limit logic so that if it is reached (and not only when it is exceeded) `pg-collectd` will submit values to Postgres. Previously if one had a batch limit of `10`, those 10 values wouldn't be submitted until the 11th value was received. Very minor logic change from `> self.batch_limit` to `>= self.batch_limit`.

# 0.1.0 - 2018-10-31

* Initial release
