//! Browser-emulating HTTP client and Wealthsimple base headers.

mod client;
mod error;
mod headers;

pub use client::{Client, Response};
pub use error::Error;
pub use headers::Headers;
