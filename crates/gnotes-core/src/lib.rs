//! gnotes core: the encrypted vault, independent of any UI.

pub mod crypto;
pub mod error;
pub mod paths;
pub mod vault;

pub use error::{Error, Result};
pub use vault::{Note, NoteMeta, Tree, Vault};
