use log::Level;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct PgCollectdConfig {
    #[serde(rename = "Connection")]
    pub connection: String,

    #[serde(default = "batch_size_default", rename = "BatchSize")]
    pub batch_size: usize,

    #[serde(default = "store_rates_default", rename = "StoreRates")]
    pub store_rates: bool,

    #[serde(default = "log_timings_default", rename = "LogTimings")]
    pub log_timings: Level,
}

fn batch_size_default() -> usize {
    100
}

fn store_rates_default() -> bool {
    true
}

fn log_timings_default() -> Level {
    Level::Debug
}
