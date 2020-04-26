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

    pub fn flush(&mut self) -> Result<(), PgError> {
        let start = Instant::now();
        let res = if let Some(client) = self.client.as_mut() {
            PgInserter::postgres_insert(client, &self.buffer[..])
        } else {
            info!("initializing new connection");
            self.last_connect = Some(start);

            let mut c = Client::connect(self.uri.as_str(), NoTls)?;
            let res = PgInserter::postgres_insert(&mut c, &self.buffer[..])?;
            self.client = Some(c);
            Ok(res)
        };

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

        self.batched = 0;
        self.buffer.clear();
        res.map(|_| ())
    }

    pub fn send_data(&mut self, data: &[u8], values: usize) -> Result<(), PgError> {
        self.buffer.extend_from_slice(&data[..]);
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

        // Simulate ten rows inserting to exceed batch
        ins.send_data(
            b"2004-10-19 10:23:54+02,plugin,plugin_instance,type_instance,type,host,metric,10.0\n",
            10,
        )
        .unwrap();
        assert_eq!(count_rows(), 1);

        // Simulate adding one row
        ins.send_data(
            b"2004-10-19 10:23:55+02,plugin,plugin_instance,type_instance,type,host,metric,10.0\n",
            1,
        )
        .unwrap();
        assert_eq!(count_rows(), 1);

        // Simulate adding nine more to cause copy insert
        ins.send_data(
            b"2004-10-19 10:23:56+02,plugin,plugin_instance,type_instance,type,host,metric,10.0\n",
            9,
        )
        .unwrap();
        assert_eq!(count_rows(), 3);
    }
}
