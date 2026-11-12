use crate::errors::PgError;
use chrono::Duration;
use log::{info, log, Level};
use postgres::{self, Client, NoTls};
use std::io::Write;
use std::time::Instant;

pub struct PgInserter {
    uri: String,
    client: Option<Client>,
    log_timings: Level,
    buffer: Vec<u8>,
    batched: usize,
    batch_limit: usize,
    last_failed_connect: Option<Instant>,
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
            last_failed_connect: None,
        }
    }

    fn postgres_insert(client: &mut Client, data: &[u8]) -> Result<u64, PgError> {
        // Name the columns, so that the CSV fields go into the correct columns.
        // The field order must be the same as the order in plugin::Submission.
        let mut writer = client.copy_in(
            "COPY collectd_metrics (time, plugin, plugin_instance, type_instance, type, host, metric, value) \
             FROM STDIN WITH (FORMAT CSV)",
        )?;
        writer.write_all(data)?;
        let rows = writer.finish()?;
        Ok(rows)
    }

    /// Connect if necessary, then copy the buffer to postgres.
    fn insert_buffer(&mut self, start: Instant) -> Result<u64, PgError> {
        let client = match self.client.as_mut() {
            Some(client) => client,
            None => {
                info!("initializing new connection");
                match Client::connect(self.uri.as_str(), NoTls) {
                    Ok(client) => self.client.insert(client),
                    Err(e) => {
                        self.last_failed_connect = Some(start);
                        return Err(e.into());
                    }
                }
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
        let res = self.insert_buffer(start);

        if let Ok(rows) = res {
            log!(
                self.log_timings,
                "inserted {} rows from {} values from {} bytes (capacity: {}) in {}ms",
                rows,
                self.batched,
                self.buffer.len(),
                self.buffer.capacity(),
                Duration::from_std(Instant::now().duration_since(start))
                    .map(|x| x.num_milliseconds())
                    .map(|x| x.to_string())
                    .unwrap_or_else(|_| String::from("<error>"))
            );
        } else {
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
        if self.batched >= self.batch_limit {
            // After a failed connect, do not try to connect again in the same
            // second. Many connect attempts do not help a server that is down.
            // The write_graphite plugin does the same. A failed insert does not
            // start this back-off, so the next batch can use a new connection.
            let too_soon = self
                .last_failed_connect
                .is_some_and(|x| x.elapsed().as_secs() == 0);
            if self.client.is_none() && too_soon && !self.buffer.is_empty() {
                self.reset_batch();
                Err(PgError::ConnectBackoff)
            } else {
                self.flush()
            }
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod test {
    use super::PgInserter;
    use crate::errors::PgError;
    use postgres::{Client, NoTls};

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

        // The next batch must not contain the rejected row. A failed insert
        // does not start the reconnect back-off, so this batch goes to postgres.
        ins.send_data(row(PLUGIN).as_bytes(), 1).unwrap();
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
    fn failed_connect_starts_back_off() {
        // Nothing listens on this port, so the connect fails.
        let uri = String::from("postgresql://collectd:hellocollectd@timescale:1/timescale_built");
        let mut ins = PgInserter::new(uri, 1, log::Level::Info);
        let err = ins
            .send_data(row("back_off_test").as_bytes(), 1)
            .unwrap_err();
        assert!(matches!(err, PgError::Postgres(_)));

        // The next batch in the same second does not try to connect.
        let err = ins
            .send_data(row("back_off_test").as_bytes(), 1)
            .unwrap_err();
        assert!(matches!(err, PgError::ConnectBackoff));
        assert!(ins.buffer.is_empty());
    }
}
