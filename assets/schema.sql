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

SELECT create_hypertable('collectd_metrics', 'time', if_not_exists => TRUE);
ALTER TABLE collectd_metrics SET (
   timescaledb.compress,
   timescaledb.compress_segmentby = 'plugin',
   timescaledb.compress_orderby = 'time'
);
SELECT add_compression_policy('collectd_metrics', INTERVAL '7 days');
SELECT add_retention_policy('collectd_metrics', INTERVAL '90 days');
