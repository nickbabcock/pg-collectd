use chrono::prelude::*;
use chrono::Duration;
use collectd_plugin::{
    self, CollectdLoggerBuilder, ConfigItem, Plugin, PluginCapabilities, PluginManager,
    PluginRegistration, Value, ValueList,
};
use crate::config::PgCollectdConfig;
use csv;
use crate::inserter::PgInserter;
use log::LevelFilter;
use parking_lot::Mutex;
use serde::Serialize;
use std::cell::Cell;
use std::error;
use std::panic::AssertUnwindSafe;

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

pub struct PgCollectd {
    inserter: AssertUnwindSafe<Mutex<PgInserter>>,
    store_rates: bool,
}

impl PluginManager for PgCollectd {
    fn name() -> &'static str {
        "pg_collectd"
    }

    fn plugins(config: Option<&[ConfigItem<'_>]>) -> Result<PluginRegistration, Box<dyn error::Error>> {
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
            inserter: AssertUnwindSafe(Mutex::new(PgInserter::new(
                config.connection,
                config.batch_size,
                config.log_timings,
            ))),
        };

        Ok(PluginRegistration::Single(Box::new(plugin)))
    }
}

impl Plugin for PgCollectd {
    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::WRITE | PluginCapabilities::FLUSH
    }

    fn write_values(&self, list: ValueList<'_>) -> Result<(), Box<dyn error::Error>> {
        // We have a thread local csv buffer that we use to prep the payload. This should be a
        // win-win:
        //  - amortize allocations: allocations only needed on new threads or new list exceeds
        //  previous capacity (should be extremely rare)
        //  - allows some concurrency as each payload can be prepped before needing to lock for a
        //  (potential) insert
        //  - Since `Vec::new` does not allocate, it's cheap to take and set from a Cell
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

            // Filter our any NaN values (they occur for the first value of a rate, as two points
            // are needed to determine a rate)
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
        let result = inserter.send_data(&v[..], len);

        // Even if we fail to insert into postgres, we should still clear out our intermediate
        // buffer so that if the db is down for a long time, it doesn't affect memory usage
        // negatively. We also want to keep the same buffer across errors as it contains
        // our amoritized allocations.
        v.clear();
        TEMP_BUF.with(|cell| cell.set(v));
        Ok(result?)
    }

    fn flush(
        &self,
        _timeout: Option<Duration>,
        _identifier: Option<&str>,
    ) -> Result<(), Box<dyn error::Error>> {
        let mut inserter = self.inserter.lock();
        inserter.flush()?;
        Ok(())
    }
}
