use chrono::prelude::*;
use collectd_plugin::{
    self, CollectdLoggerBuilder, ConfigItem, Plugin, PluginCapabilities, PluginManager,
    PluginRegistration, Value, ValueList,
};
use csv;
use failure::Error;
use inserter::PgInserter;
use log::LevelFilter;
use parking_lot::Mutex;
use std::cell::Cell;
use failure::ResultExt;

#[derive(Serialize, Debug)]
struct Submission<'a> {
    timestamp: DateTime<Utc>,
    plugin: &'a str,
    plugin_instance: Option<&'a str>,
    type_: &'a str,
    type_instance: Option<&'a str>,
    host: &'a str,
    metric: &'a str,
    value: Value,
}

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
struct PgCollectdConfig {
    #[serde(rename = "Connection")]
    connection: String,

    #[serde(default = "batch_size_default", rename = "BatchSize")]
    batch_size: usize,

    #[serde(default = "store_rates_default", rename = "StoreRates")]
    store_rates: bool
}

fn batch_size_default() -> usize {
    100
}

fn store_rates_default() -> bool {
    true
}

pub struct PgCollectd {
    inserter: Mutex<PgInserter>,
    store_rates: bool
}

impl PluginManager for PgCollectd {
    fn name() -> &'static str {
        "pg_collectd"
    }

    fn plugins(config: Option<&[ConfigItem]>) -> Result<PluginRegistration, Error> {
        // hook rust logging into collectd's logging
        CollectdLoggerBuilder::new()
            .prefix_plugin::<Self>()
            .filter_level(LevelFilter::Info)
            .try_init()
            .expect("really the only thing that should create a logger");

        let config: PgCollectdConfig =
            collectd_plugin::de::from_collectd(config.unwrap_or_else(Default::default))?;

        let plugin = PgCollectd {
            store_rates: config.store_rates,
            inserter: Mutex::new(PgInserter::new(config.connection, config.batch_size)),
        };

        Ok(PluginRegistration::Single(Box::new(plugin)))
    }
}

impl Plugin for PgCollectd {
    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::WRITE
    }

    fn write_values(&self, list: ValueList) -> Result<(), Error> {
        thread_local!(static TEMP_BUF: Cell<Vec<u8>> = Cell::new(Vec::new()));
        let mut v = TEMP_BUF.with(|cell| cell.take());
        let len = list.values.len();

        {
            let mut wtr = csv::WriterBuilder::new()
                .has_headers(false)
                .from_writer(&mut v);

            let values = if self.store_rates {
                list.rates()
            } else {
                Ok(::std::borrow::Cow::Borrowed(&list.values))
            }?;

            for value in values.iter().filter(|x| !x.value.is_nan()) {
                let submission = Submission {
                    timestamp: list.time,
                    plugin: list.plugin,
                    plugin_instance: list.plugin_instance,
                    type_instance: list.type_instance,
                    type_: list.type_,
                    host: list.host,
                    metric: value.name,
                    value: value.value,
                };

                if let Err(ref e) = wtr.serialize(submission) {
                    warn!("unable to serialize to csv for postgres: {}", e);
                }
            }
        }

        let mut inserter = self.inserter.lock();
        inserter.send_data(&v[..], len)
            .context("unable to insert into postgres")?;

        v.clear();
        TEMP_BUF.with(|cell| cell.set(v));

        Ok(())
    }
}
