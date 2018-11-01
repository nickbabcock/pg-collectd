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
- Not as feature rich (eg: currently no support TLS connections)
- Only tested on a limited subset of collectds (though it may work on other
  versions depending on if collectd changed its C API)
- Only distributed as debs or source (eg: no rpms / apt repository)

## Compatibility

This repo is tested on the following (though compatibility isn't limited to):

- collectd 5.4 (Ubuntu 14.04)
- collectd 5.5 (Ubuntu 16.04)
- collectd 5.7 (Ubuntu 18.04)
- collectd 5.8 (Ubuntu 18.10)

Postgre 7.4 or later is required.

## Quick start

Below is a sample collectd configuration

```
LoadPlugin pg_collectd
<Plugin pg_collectd>
    BatchSize 1000
    Connection "postgresql://<user>:<password>@<host>:<port>/<db>"
    StoreRates true
</Plugin>
```

- BatchSize: number of values to batch (eg: rows in the csv) before copying them to the database. Default is 100, which is extremely conservative. Test what is appropriate for you, but 500 to 1000 works well for me. Note that it is possible for the number of rows inserted to not be exactly equal to batch size, as `NaN` rates are not stored and some metrics given to write contain more than one value.
- Connection (see [postgres connection uri documentation](https://www.postgresql.org/docs/10/static/libpq-connect.html#id-1.7.3.8.3.6))
- StoreRates: Controls whether DERIVE and COUNTER metrics are converted to a rate before sending. Default is true.

## Schema

Given how young and opinionated this plugin is, there is not flexibility in how the structure of the data.

The only officially supported schema (the one that is tested against) is the one below:

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

It is recommended that the user created for this plugin be only given `INSERT` permissions on `collectd_metrics`

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

To build the repo for collectd:

```
COLLECTD_VERSION=5.7 cargo build --release
```

The resulting `./target/release/libpg_collectd.so` should be copied (locally
or remotely) to `/usr/lib/collectd/pg_collectd.so`
