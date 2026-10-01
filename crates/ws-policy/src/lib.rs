//! Permission levels, limits, budgets, and the kill switch.
//!
//! `Guard` is the only path from an order request to a broker.

mod budget;
mod check;
mod error;
mod guard;
mod kill;
mod ticket;
