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
        let mut writer = client.copy_in("COPY collectd_metrics FROM STDIN WITH (FORMAT CSV)")?;
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

    pub fn flush(&mut self) -> Result<(), PgError> {
        if self.buffer.is_empty() {
            self.batched = 0;
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
            self.client = None
        }

        // Clear the batch also when the insert fails. If we keep it, postgres
        // can reject the same rows again and again, and the buffer gets larger.
        self.batched = 0;
        self.buffer.clear();
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
                self.batched = 0;
                self.buffer.clear();
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
    use postgres::{Client, NoTls};

    const CONNECTION: &str = "postgresql://collectd:hellocollectd@timescale/timescale_built2";
    const CONNECTION_E2E: &str = "postgresql://collectd:hellocollectd@timescale/timescale_built";

    fn count_rows() -> i64 {
        Client::connect(
            "postgresql://postgres:my_rust_test@timescale/timescale_built2",
            NoTls,
        )
        .unwrap()
        .query_one("SELECT COUNT(*) FROM collectd_metrics", &[])
        .unwrap()
        .get(0)
    }

    #[test]
    fn insert_values() {
        let mut ins = PgInserter::new(String::from(CONNECTION), 10, log::Level::Info);
        let start = count_rows();

        // Simulate ten rows inserting to exceed batch
        ins.send_data(
            b"2004-10-19 10:23:54+02,plugin,plugin_instance,type_instance,type,host,metric,10.0\n",
            10,
        )
        .unwrap();
        assert_eq!(count_rows(), start + 1);

        // Simulate adding one row
        ins.send_data(
            b"2004-10-19 10:23:55+02,plugin,plugin_instance,type_instance,type,host,metric,10.0\n",
            1,
        )
        .unwrap();
        assert_eq!(count_rows(), start + 1);

        // Simulate adding nine more to cause copy insert
        ins.send_data(
            b"2004-10-19 10:23:56+02,plugin,plugin_instance,type_instance,type,host,metric,10.0\n",
            9,
        )
        .unwrap();
        assert_eq!(count_rows(), start + 3);
    }

    #[test]
    fn failed_batch_is_discarded() {
        // Use a different database, so that the row counts in insert_values do not change.
        let mut ins = PgInserter::new(String::from(CONNECTION_E2E), 1, log::Level::Info);

        // The connection is good, but postgres rejects the row.
        let bad_row = b"not-a-time,plugin,plugin_instance,type_instance,type,host,metric,10.0\n";
        assert!(ins.send_data(bad_row, 1).is_err());

        // Wait until the reconnect back-off stops.
        std::thread::sleep(std::time::Duration::from_millis(1100));

        // The next batch must not contain the rejected row.
        let good_row = format!(
            "{},plugin,plugin_instance,type_instance,type,host,metric,10.0\n",
            chrono::Utc::now().to_rfc3339()
        );
        ins.send_data(good_row.as_bytes(), 1).unwrap();
    }
}
