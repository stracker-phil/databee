//! Long-running background loops restart after a panic, so one bug costs a moment of that loop, not the
//! feature for the rest of the day (docs/adr/0017). Socket and Stage connections need nothing extra: each
//! runs in a task of its own, so a panic fails only that request.

use std::future::Future;
use std::time::Duration;

const RESTART_DELAY: Duration = Duration::from_secs(1);

/// Runs `task` on the async runtime and starts it again whenever it panics. A task that returns is done.
pub fn supervise<F, Fut>(name: &'static str, task: F)
where
    F: FnMut() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    tauri::async_runtime::spawn(restart_on_panic(name, task, RESTART_DELAY));
}

async fn restart_on_panic<F, Fut>(name: &'static str, mut task: F, delay: Duration)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    loop {
        match tokio::spawn(task()).await {
            Err(e) if e.is_panic() => {
                log::error!("The {name} task panicked; restarting it");
                tokio::time::sleep(delay).await;
            }
            _ => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn a_task_that_panics_is_started_again() {
        let runs = Arc::new(Mutex::new(Vec::new()));
        let record = runs.clone();

        restart_on_panic(
            "test",
            move || {
                let record = record.clone();
                async move {
                    let first = {
                        let mut runs = record.lock().unwrap();
                        runs.push("started");
                        runs.len() == 1
                    };
                    if first {
                        panic!("first run fails");
                    }
                    record.lock().unwrap().push("finished");
                }
            },
            Duration::ZERO,
        )
        .await;

        assert_eq!(
            *runs.lock().unwrap(),
            vec!["started", "started", "finished"]
        );
    }

    #[tokio::test]
    async fn a_task_that_returns_is_not_started_again() {
        let runs = Arc::new(Mutex::new(Vec::new()));
        let record = runs.clone();

        restart_on_panic(
            "test",
            move || {
                let record = record.clone();
                async move {
                    record.lock().unwrap().push("ran");
                }
            },
            Duration::ZERO,
        )
        .await;

        assert_eq!(*runs.lock().unwrap(), vec!["ran"]);
    }
}
