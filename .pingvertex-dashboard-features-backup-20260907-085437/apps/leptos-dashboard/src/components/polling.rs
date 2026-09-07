use gloo_timers::future::TimeoutFuture;
use leptos::prelude::on_cleanup;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

const BASE_DELAY: Duration = Duration::from_secs(15);
const HIDDEN_DELAY: Duration = Duration::from_secs(60);
const MAX_FAILURE_DELAY: Duration = Duration::from_secs(300);

fn page_visible() -> bool {
    web_sys::window()
        .and_then(|window| window.document())
        .map(|document| !document.hidden())
        .unwrap_or(true)
}

/// Starts a cancellable, visibility-aware polling loop.
///
/// The callback is never run concurrently with itself. Successful polls return
/// to the normal cadence; failures back off exponentially up to five minutes.
/// Hidden tabs do not make API requests and are checked again once per minute.
pub fn start<F, Fut>(mut poll: F)
where
    F: FnMut() -> Fut + 'static,
    Fut: std::future::Future<Output = Result<(), ()>> + 'static,
{
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_for_cleanup = cancelled.clone();
    on_cleanup(move || cancelled_for_cleanup.store(true, Ordering::Relaxed));

    leptos::task::spawn_local(async move {
        let mut failure_delay = BASE_DELAY;

        loop {
            let delay = if page_visible() {
                failure_delay
            } else {
                HIDDEN_DELAY
            };
            TimeoutFuture::new(delay.as_millis().min(u32::MAX as u128) as u32).await;

            if cancelled.load(Ordering::Relaxed) {
                break;
            }

            if !page_visible() {
                continue;
            }

            match poll().await {
                Ok(()) => failure_delay = BASE_DELAY,
                Err(()) => {
                    failure_delay = (failure_delay * 2).min(MAX_FAILURE_DELAY);
                }
            }
        }
    });
}
