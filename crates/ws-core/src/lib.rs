//! Domain types shared by every crate. No IO.

mod account;
mod activity;
mod clock;
mod error;
mod ids;
mod money;
mod order;
mod position;
mod quote;
mod security;

pub use account::{Account, Management, Registration};
pub use activity::Activity;
pub use clock::{Clock, FixedClock, SystemClock};
pub use error::Error;
pub use ids::{AccountId, IdempotencyKey, IdentityId, OrderId, SecurityId};
pub use money::{Currency, Money};
pub use order::{Kind, Order, Quantity, Side, Tif};
pub use position::Position;
pub use quote::{MarketStatus, Quote};
pub use security::Security;
