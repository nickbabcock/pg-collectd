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
