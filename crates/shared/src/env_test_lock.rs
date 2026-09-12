#![cfg(test)]

use tokio::sync::Mutex;

pub(crate) static ENV_MUTEX: Mutex<()> = Mutex::const_new(());
