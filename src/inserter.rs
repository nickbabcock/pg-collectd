use chrono::Duration;
use postgres::{self, Connection, TlsMode};
use std::time::Instant;

#[derive(Default)]
pub struct PgInserter {
    uri: String,
    connection: Option<Connection>,
    buffer: Vec<u8>,
    batched: usize,
    batch_limit: usize,
}

impl PgInserter {
    pub fn new(uri: String, batch_limit: usize) -> Self {
        PgInserter {
            uri,
            batch_limit,
            ..Default::default()
        }
    }

    fn postgres_insert(conn: &Connection, mut data: &[u8]) -> Result<u64, postgres::Error> {
        let stmt = conn.prepare_cached("COPY collectd_metrics FROM STDIN WITH (FORMAT CSV)")?;
        stmt.copy_in(&[], &mut data)
    }

    pub fn send_data(&mut self, data: &[u8], values: usize) -> Result<(), postgres::Error> {
        self.buffer.extend_from_slice(&data[..]);
        self.batched += values;
        if self.batched > self.batch_limit {
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
                info!(
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
        } else {
            Ok(())
        }
    }
}
