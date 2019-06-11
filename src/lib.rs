#[macro_use]
extern crate collectd_plugin;
#[macro_use]
extern crate failure;
extern crate postgres;
#[macro_use]
extern crate log;
extern crate serde;
extern crate chrono;
extern crate csv;
extern crate parking_lot;

mod config;
mod inserter;
mod plugin;

collectd_plugin!(plugin::PgCollectd);
