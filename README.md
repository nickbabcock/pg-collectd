[![ci](https://github.com/nickbabcock/pg-collectd/actions/workflows/ci.yml/badge.svg)](https://github.com/nickbabcock/pg-collectd/actions/workflows/ci.yml)

# pg-collectd

pg-collectd provides an alternative and opinionated postgres collectd writer
plugin, where flexibility is traded in for performance and ease of use. A quick
rundown.

- No dependencies (other than collectd). pg-collectd utilizes the (unofficial) [collectd rust plugin](https://github.com/nickbabcock/collectd-rust-plugin) for low cost binding to collectd's C API. The [pure rust postgres
  driver](https://github.com/sfackler/rust-postgres) is statically compiled
  into the plugin. No need to rely on libpq.
- Simplified insertion as the data is expanded and denormalized, so instead of
  writing a function that receives an array of values / identifiers, these are
  expanded so everything fits into a single table and columns contain single
  values (not arrays).
- a 4x reduction in db cpu usage compared to using [collectd's default postgres writer + setup](https://github.com/collectd/collectd/blob/92c3b2ed5f8e49737e29b11244585960a3478494/contrib/postgresql/collectd_insert.sql) (a conservative estimate)

Here are the downsides:

- Not an officially supported collectd plugin
- Not as feature rich (eg: currently no support for TLS connections / does not support custom table names)
- Only tested on a limited subset of collectds (though it may work on other
  versions depending on if collectd changed its C API)
- Only distributed as debs or source (eg: no rpms / apt repository)

## Compatibility

- Collectd 5.7+
- Postgres 7.4+

## Installation

First we must set up the database with the following schema:

```sql
CREATE TABLE IF NOT EXISTS collectd_metrics (
   time TIMESTAMPTZ NOT NULL,
   plugin TEXT,
   plugin_instance TEXT,
   type_instance TEXT,
   type TEXT,
   host TEXT,
   metric TEXT,
   value DOUBLE PRECISION
);
```

- (Optional) If using the TimescaleDB extension for postgres, the statements below contains provide good defaults

```sql
SELECT create_hypertable('collectd_metrics', 'time', chunk_time_interval => interval '1 day');
ALTER TABLE collectd_metrics SET (
   timescaledb.compress,
   timescaledb.compress_segmentby = 'plugin',
   timescaledb.compress_orderby = 'time'
);
SELECT add_compression_policy('collectd_metrics', INTERVAL '7 days');
SELECT add_retention_policy('collectd_metrics', INTERVAL '90 days');
```

- Create a user that only has `INSERT` permissions on `collectd_metrics`:

```sql
CREATE USER collectd WITH PASSWORD 'xxx';
GRANT INSERT ON collectd_metrics TO collectd;
```

- Download the appropriate package from the [latest
  release](https://github.com/nickbabcock/pg-collectd/releases/latest) (see
  the compatibility list shown earlier)
- Install with `dpkg -i pg-collectd-*.deb`
- Edit collectd configuration (eg: `/etc/collectd/collectd.conf`)

```
LoadPlugin pg_collectd
<Plugin pg_collectd>
    BatchSize 1000
    Connection "postgresql://<user>:<password>@<host>:<port>/<db>"
    StoreRates true
    LogTimings INFO
</Plugin>
```

- Restart collectd

Not using Ubuntu / Debian? No problem, [build from source](#building).

## Configuration Option

- BatchSize: number of values to batch (eg: rows in the csv) before copying them to the database. Default is 100, which is extremely conservative. Test what is appropriate for you, but 500 to 1000 works well for me. Note that it is possible for the number of rows inserted to not be exactly equal to batch size, as `NaN` rates are not stored and some metrics given to write contain more than one value.
- Connection (see [postgres connection uri documentation](https://www.postgresql.org/docs/10/static/libpq-connect.html#id-1.7.3.8.3.6))
- StoreRates: Controls whether DERIVE and COUNTER metrics are converted to a rate before sending. Default is true.
- LogTimings: The level at which to log performance timings. The default is `DEBUG` to cut down on potential log spam, though there is no problem setting it to `INFO` (or `WARN` / `ERROR` for that matter), as only a single line is logged per batched insert.

## Performance Secret Sauce

The original postgres writer for collectd works by having a long lasting
transaction writing many individual statements committed at `CommitInterval`
seconds. [A quote from postgres's official
documentation](https://www.postgresql.org/docs/9.2/static/populate.html) leads
us to ponder low hanging fruit:

> Note that loading a large number of rows using COPY is almost always faster
> than using INSERT, even if PREPARE is used and multiple insertions are
> batched into a single transaction.

To take advantage of this, pg-collectd batches up a certain number of values
(`BatchSize`), and then formats those values as part of a in-memory CSV file
that can `COPY` over to postgres. What's nice is that memory allocations are
amortized such that over time, no memory is allocated for the in-memory CSV
file, only the CPU time for formatting the CSV is needed.

## Building

To build the repo for collectd, ensure you have [Rust
installed](https://rustup.rs/) and then execute the build process:

```
cargo build --release
```

The resulting `./target/release/libpg_collectd.so` should be copied (locally
or remotely) to `/usr/lib/collectd/pg_collectd.so`
