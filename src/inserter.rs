use crate::errors::PgError;
use chrono::Duration;
use log::{info, log, Level};
use postgres::{self, error::Severity, Client, NoTls};
use std::io::Write;
use std::time::Instant;

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
                self.last_connect = Some(start);
                self.client
                    .insert(Client::connect(self.uri.as_str(), NoTls)?)
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
            // Keep the connection only if postgres rejected the data. For all
            // other errors, for example a server restart, connect again.
            let rejected = res.as_ref().err().is_some_and(is_data_rejection);
            if !rejected || self.client.as_ref().is_some_and(|c| c.is_closed()) {
                self.client = None
            }
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
            // If we are not connected and we've recently allocated a connection then we should not
            // even try and insert as spamming connection attempts helps no one. The impetus for
            // this is taken from the write_graphite plugin
            let too_soon = self
                .last_connect
                .map(|x| Instant::now().duration_since(x).as_secs() == 0);
            if self.client.is_none() && too_soon.unwrap_or(false) {
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

/// Return true if postgres rejected the data with an error that does not
/// close the connection. A FATAL error, for example, closes the connection.
fn is_data_rejection(err: &PgError) -> bool {
    match err {
        PgError::Postgres(e) => {
            e.as_db_error().and_then(|db| db.parsed_severity()) == Some(Severity::Error)
        }
        _ => false,
    }
}

#[cfg(test)]
mod test {
    use super::PgInserter;
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
        // Use a different database, so that the row counts in insert_values do not change.
        const PLUGIN: &str = "failed_batch_test";
        let mut ins = PgInserter::new(String::from(CONNECTION_E2E), 1, log::Level::Info);
        let start = count_rows("timescale_built", PLUGIN);

        // The connection is good, but postgres rejects the row.
        let bad_row =
            format!("not-a-time,{PLUGIN},plugin_instance,type_instance,type,host,metric,10.0\n");
        assert!(ins.send_data(bad_row.as_bytes(), 1).is_err());

        // Keep the connection, so that the reconnect back-off does not discard the next batch.
        assert!(ins.client.is_some());

        // The next batch must not contain the rejected row.
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
}
