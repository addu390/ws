mod error;
mod file;
#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures;

pub use error::Error;
pub use file::{Locked, append, private_dir, read, read_json, remove, remove_dir, write, write_json};
