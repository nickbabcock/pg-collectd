#[macro_use]
extern crate collectd_plugin;
extern crate postgres;
#[macro_use]
extern crate log;
extern crate chrono;
extern crate csv;
extern crate parking_lot;
extern crate serde;

mod config;
mod inserter;
mod plugin;

collectd_plugin!(plugin::PgCollectd);
