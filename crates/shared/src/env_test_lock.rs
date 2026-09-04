//! A single process-wide mutex shared by every `#[cfg(test)]` module in
//! this crate that mutates process environment variables (currently
//! `ssrf::tests` and `cookie::tests`, both of which read/write
//! `ENVIRONMENT`). `std::env::set_var`/`remove_var` affect the whole
//! process, so two such test modules each serializing themselves with
//! their own private `Mutex` does NOT prevent them from racing against
//! each other when `cargo test` runs them concurrently on different
//! threads. Sharing one lock here makes them mutually exclusive too.
#![cfg(test)]

use std::sync::Mutex;

pub(crate) static ENV_MUTEX: Mutex<()> = Mutex::new(());
