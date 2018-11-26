# Unreleased - TBA

* Update to collectd-plugin 0.8.4 from 0.8.1, which fixes segfaults on plugin flush and shutdown
* Add: configurable log level for timing data using the LogTimings configuration option (an example of a log timing: "inserted 1000 rows from 1000 values from 86403 bytes (capacity: 157696) in 54ms")
* Fix batch limit logic so that if it is reached (and not only when it is exceeded) `pg-collectd` will submit values to Postgres. Previously if one had a batch limit of `10`, those 10 values wouldn't be submitted until the 11th value was received. Very minor logic change from `> self.batch_limit` to `>= self.batch_limit`.

# 0.1.0 - 2018-10-31

* Initial release
