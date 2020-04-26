use std::error::Error;
use std::fmt;
use std::io;

#[derive(Debug)]
pub enum PgError {
    ConnectBackoff,
    Postgres(postgres::Error),
    PostgresIo(io::Error),
}

impl fmt::Display for PgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PgError::ConnectBackoff => write!(f, "waiting until connect backoff to try again"),
            PgError::Postgres(ref e) => write!(f, "postgres error: {}", e),
            PgError::PostgresIo(ref e) => write!(f, "postgres io error: {}", e),
        }
    }
}

impl Error for PgError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            PgError::Postgres(ref e) => Some(e),
            PgError::PostgresIo(ref e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for PgError {
    fn from(error: io::Error) -> Self {
        PgError::PostgresIo(error)
    }
}

impl From<postgres::Error> for PgError {
    fn from(error: postgres::Error) -> Self {
        PgError::Postgres(error)
    }
}
