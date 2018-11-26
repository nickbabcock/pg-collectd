use chrono::Duration;
use log::Level;
use postgres::{self, Connection, TlsMode};
use std::time::Instant;

pub struct PgInserter {
    uri: String,
    connection: Option<Connection>,
    log_timings: Level,
    buffer: Vec<u8>,
    batched: usize,
    batch_limit: usize,
}

impl PgInserter {
    pub fn new(uri: String, batch_limit: usize, log_timings: Level) -> Self {
        PgInserter {
            uri,
            batch_limit,
            log_timings,
            connection: None,
            buffer: Vec::new(),
            batched: 0,
        }
    }

    fn postgres_insert(conn: &Connection, mut data: &[u8]) -> Result<u64, postgres::Error> {
        let stmt = conn.prepare_cached("COPY collectd_metrics FROM STDIN WITH (FORMAT CSV)")?;
        stmt.copy_in(&[], &mut data)
    }

    pub fn flush(&mut self) -> Result<(), postgres::Error> {
        let start = Instant::now();
        let res = if let Some(ref conn) = self.connection {
            PgInserter::postgres_insert(conn, &self.buffer[..])
        } else {
            info!("initializing new connection");
            let c = Connection::connect(self.uri.as_str(), TlsMode::None)?;
            let res = PgInserter::postgres_insert(&c, &self.buffer[..])?;
            self.connection = Some(c);
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
            self.connection = None
        }

        self.batched = 0;
        self.buffer.clear();
        res.map(|_| ())
    }

    pub fn send_data(&mut self, data: &[u8], values: usize) -> Result<(), postgres::Error> {
        self.buffer.extend_from_slice(&data[..]);
        self.batched += values;
        if self.batched >= self.batch_limit {
            self.flush()
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod test {
    use super::PgInserter;
    use postgres::{Connection, TlsMode};

    const CONNECTION: &str = "postgresql://collectd:hellocollectd@timescale/timescale_built2";

    fn count_rows() -> i64 {
        let query = Connection::connect(
            "postgresql://postgres:my_rust_test@timescale/timescale_built2",
            TlsMode::None,
        ).unwrap()
        .query("SELECT COUNT(*) FROM collectd_metrics", &[])
        .unwrap();
        let record = query.get(0);
        record.get(0)
    }

    #[test]
    fn insert_values() {
        let mut ins = PgInserter::new(String::from(CONNECTION), 10, log::Level::Info);

        // Simulate ten rows inserting to exceed batch
        ins.send_data(
            b"2004-10-19 10:23:54+02,plugin,plugin_instance,type_instance,type,host,metric,10.0\n",
            10,
        ).unwrap();
        assert_eq!(count_rows(), 1);

        // Simulate adding one row
        ins.send_data(
            b"2004-10-19 10:23:55+02,plugin,plugin_instance,type_instance,type,host,metric,10.0\n",
            1,
        ).unwrap();
        assert_eq!(count_rows(), 1);

        // Simulate adding nine more to cause copy insert
        ins.send_data(
            b"2004-10-19 10:23:56+02,plugin,plugin_instance,type_instance,type,host,metric,10.0\n",
            9,
        ).unwrap();
        assert_eq!(count_rows(), 3);
    }
}
