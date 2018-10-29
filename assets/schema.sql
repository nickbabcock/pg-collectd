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
