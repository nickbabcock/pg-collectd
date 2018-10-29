#[macro_use]
extern crate collectd_plugin;
extern crate failure;
extern crate postgres;
#[macro_use]
extern crate log;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate chrono;
extern crate csv;
extern crate parking_lot;

mod config;
mod inserter;
mod plugin;

collectd_plugin!(plugin::PgCollectd);
