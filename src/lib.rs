#[macro_use]
extern crate collectd_plugin;

#[macro_use]
extern crate log;





mod config;
mod inserter;
mod plugin;

collectd_plugin!(plugin::PgCollectd);
