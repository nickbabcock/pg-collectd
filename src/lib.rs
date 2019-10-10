mod config;
mod inserter;
mod plugin;

collectd_plugin::collectd_plugin!(plugin::PgCollectd);
