use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("wrong password or not a gnotes vault")]
    WrongPassword,
    #[error("a vault already exists at this location")]
    VaultExists,
    #[error("vault file not found")]
    VaultNotFound,
    #[error("invalid name: {0}")]
    InvalidName(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("already exists: {0}")]
    AlreadyExists(String),
    #[error("vault was created by a newer gnotes version (schema {0})")]
    UnsupportedSchema(i64),
    #[error("crypto error: {0}")]
    Crypto(String),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
