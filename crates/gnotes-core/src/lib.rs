//! gnotes core: the encrypted vault, independent of any UI.

pub mod attachments;
pub mod crypto;
pub mod error;
pub mod paths;
pub mod search;
pub mod tags;
pub mod transfer;
pub mod vault;

pub use attachments::AttachmentMeta;
pub use error::{Error, Result};
pub use search::{LineMatch, SearchHit, SearchResults};
pub use tags::TagCount;
pub use vault::{Note, NoteMeta, Tree, Vault};
