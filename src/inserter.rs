use crate::errors::PgError;
use log::{info, log, warn, Level};
use postgres::{self, Client, NoTls};
use std::io::Write;
use std::time::Instant;

/// The table columns, in the order of the CSV fields. A test in plugin.rs
/// makes sure that plugin::Submission writes its fields in this order.
pub(crate) const COLUMNS: &str =
    "time, plugin, plugin_instance, type_instance, type, host, metric, value";

pub struct PgInserter {
    uri: String,
    client: Option<Client>,
    log_timings: Level,
    buffer: Vec<u8>,
    batched: usize,
    batch_limit: usize,
    last_connect: Option<Instant>,
}

impl PgInserter {
    pub fn new(uri: String, batch_limit: usize, log_timings: Level) -> Self {
        PgInserter {
            uri,
            batch_limit,
            log_timings,
            client: None,
            buffer: Vec::new(),
            batched: 0,
            last_connect: None,
        }
    }

    fn postgres_insert(client: &mut Client, data: &[u8]) -> Result<u64, PgError> {
        // Name the columns, so that the CSV fields go into the correct columns.
        let statement = format!("COPY collectd_metrics ({COLUMNS}) FROM STDIN WITH (FORMAT CSV)");
        let mut writer = client.copy_in(statement.as_str())?;
        writer.write_all(data)?;
        let rows = writer.finish()?;
        Ok(rows)
    }

    /// Connect if necessary, then copy the buffer to postgres.
    fn insert_buffer(&mut self) -> Result<u64, PgError> {
        let client = match self.client.as_mut() {
            Some(client) => client,
            None => {
                info!("initializing new connection");
                let result = Client::connect(self.uri.as_str(), NoTls);

                // Record the time after the attempt, because it can take many seconds to connect.
                self.last_connect = Some(Instant::now());
                self.client.insert(result?)
            }
        };

        PgInserter::postgres_insert(client, &self.buffer)
    }

    fn reset_batch(&mut self) {
        self.batched = 0;
        self.buffer.clear();
    }

    pub fn flush(&mut self) -> Result<(), PgError> {
        if self.buffer.is_empty() {
            self.reset_batch();
            return Ok(());
        }

        let start = Instant::now();
        let res = self.insert_buffer();

        if let Ok(rows) = res {
            log!(
                self.log_timings,
                "inserted {} rows from {} values from {} bytes (capacity: {}) in {}ms",
                rows,
                self.batched,
                self.buffer.len(),
                self.buffer.capacity(),
                start.elapsed().as_millis()
            );
        } else {
            warn!("discarded {} values", self.batched);

            // Connect again for the next batch. The connection can be bad, for
            // example after a server restart.
            self.client = None
        }

        // Clear the batch also when the insert fails. If we keep it, postgres
        // can reject the same rows again and again, and the buffer becomes larger.
        self.reset_batch();
        res.map(|_| ())
    }

    pub fn send_data(&mut self, data: &[u8], values: usize) -> Result<(), PgError> {
        self.buffer.extend_from_slice(data);
        self.batched += values;
        if self.batched < self.batch_limit {
            return Ok(());
        }

        // Connect a maximum of one time each second. If the server is down, it
        // does not help to try to connect many times. The write_graphite plugin
        // does the same. Until the second ends, keep the batch in memory. The
        // next batch after that time sends it. A flush from collectd sends it
        // immediately, because collectd also flushes when it stops.
        let too_soon = self
            .last_connect
            .is_some_and(|x| x.elapsed().as_secs() == 0);
        if self.client.is_none() && too_soon {
            return Ok(());
        }

        self.flush()
    }
}

#[cfg(test)]
mod test {
    use super::PgInserter;
    use crate::errors::PgError;
    use postgres::{Client, NoTls};
    use std::time::Duration;

    const CONNECTION: &str = "postgresql://collectd:hellocollectd@timescale/timescale_built2";
    const CONNECTION_E2E: &str = "postgresql://collectd:hellocollectd@timescale/timescale_built";

    fn admin(db: &str) -> Client {
        let uri = format!("postgresql://postgres:my_rust_test@timescale/{db}");
        Client::connect(&uri, NoTls).unwrap()
    }

    /// Count the rows that have this plugin name.
    fn count_rows(db: &str, plugin: &str) -> i64 {
        admin(db)
            .query_one(
                "SELECT COUNT(*) FROM collectd_metrics WHERE plugin = $1",
                &[&plugin],
            )
            .unwrap()
            .get(0)
    }

    fn row(plugin: &str) -> String {
        format!(
            "{},{plugin},plugin_instance,type_instance,type,host,metric,10.0\n",
            chrono::Utc::now().to_rfc3339()
        )
    }

    #[test]
    fn insert_values() {
        let mut ins = PgInserter::new(String::from(CONNECTION), 10, log::Level::Info);
        let start = count_rows("timescale_built2", "plugin");

        // Simulate ten rows inserting to exceed batch
        ins.send_data(
            b"2004-10-19 10:23:54+02,plugin,plugin_instance,type_instance,type,host,metric,10.0\n",
            10,
        )
        .unwrap();
        assert_eq!(count_rows("timescale_built2", "plugin"), start + 1);

        // Simulate adding one row
        ins.send_data(
            b"2004-10-19 10:23:55+02,plugin,plugin_instance,type_instance,type,host,metric,10.0\n",
            1,
        )
        .unwrap();
        assert_eq!(count_rows("timescale_built2", "plugin"), start + 1);

        // Simulate adding nine more to cause copy insert
        ins.send_data(
            b"2004-10-19 10:23:56+02,plugin,plugin_instance,type_instance,type,host,metric,10.0\n",
            9,
        )
        .unwrap();
        assert_eq!(count_rows("timescale_built2", "plugin"), start + 3);
    }

    #[test]
    fn failed_batch_is_discarded() {
        const PLUGIN: &str = "failed_batch_test";
        let mut ins = PgInserter::new(String::from(CONNECTION_E2E), 1, log::Level::Info);
        let start = count_rows("timescale_built", PLUGIN);

        // The connection is good, but postgres rejects the row.
        let bad_row =
            format!("not-a-time,{PLUGIN},plugin_instance,type_instance,type,host,metric,10.0\n");
        assert!(ins.send_data(bad_row.as_bytes(), 1).is_err());

        // The plugin can keep the next batch in memory until the connect limit
        // ends, so flush it. Only the good row must go to postgres.
        ins.send_data(row(PLUGIN).as_bytes(), 1).unwrap();
        ins.flush().unwrap();
        assert_eq!(count_rows("timescale_built", PLUGIN), start + 1);
    }

    #[test]
    fn closed_connection_is_dropped() {
        const APP: &str = "pg_collectd_closed_connection_test";
        let uri = format!("{CONNECTION_E2E}?application_name={APP}");
        let mut ins = PgInserter::new(uri, 1, log::Level::Info);
        ins.send_data(row("closed_connection_test").as_bytes(), 1)
            .unwrap();

        // Stop the server process for this connection, and wait until it stops.
        let stopped: bool = admin("timescale_built")
            .query_one(
                "SELECT pg_terminate_backend(pid, 5000) FROM pg_stat_activity \
                 WHERE application_name = $1",
                &[&APP],
            )
            .unwrap()
            .get(0);
        assert!(stopped);

        // The insert fails, and the plugin must connect again for the next batch.
        assert!(ins
            .send_data(row("closed_connection_test").as_bytes(), 1)
            .is_err());
        assert!(ins.client.is_none());
    }

    #[test]
    fn connect_rate_is_limited() {
        // Nothing listens on this port, so the connect fails.
        let uri = String::from("postgresql://collectd:hellocollectd@timescale:1/timescale_built");
        let mut ins = PgInserter::new(uri, 1, log::Level::Info);
        let err = ins
            .send_data(row("rate_limit_test").as_bytes(), 1)
            .unwrap_err();
        assert!(matches!(err, PgError::Postgres(_)));

        // In the same second, the plugin keeps the batch and does not connect.
        let before = ins.last_connect;
        ins.send_data(row("rate_limit_test").as_bytes(), 1).unwrap();
        assert_eq!(ins.last_connect, before);
        assert!(!ins.buffer.is_empty());

        // After the second ends, the plugin tries to connect again.
        std::thread::sleep(Duration::from_secs(1));
        assert!(ins.send_data(row("rate_limit_test").as_bytes(), 1).is_err());
        assert!(ins.last_connect.unwrap() > before.unwrap());
        assert!(ins.buffer.is_empty());
    }
}
