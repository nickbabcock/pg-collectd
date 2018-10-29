#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct PgCollectdConfig {
    #[serde(rename = "Connection")]
    pub connection: String,

    #[serde(default = "batch_size_default", rename = "BatchSize")]
    pub batch_size: usize,

    #[serde(default = "store_rates_default", rename = "StoreRates")]
    pub store_rates: bool,
}

fn batch_size_default() -> usize {
    100
}

fn store_rates_default() -> bool {
    true
}
