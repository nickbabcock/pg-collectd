ALTER TABLE collectd_metrics SET (
   timescaledb.compress,
   timescaledb.compress_segmentby = 'plugin',
   timescaledb.compress_orderby = 'time'
);
SELECT add_compression_policy('collectd_metrics', INTERVAL '7 days');
SELECT add_retention_policy('collectd_metrics', INTERVAL '90 days');
