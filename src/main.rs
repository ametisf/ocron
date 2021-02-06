#![cfg_attr(test, feature(test))]

use crate::queue::Queue;
use anyhow::{Context, Result};
use chrono::prelude::*;
use config::Config;
use std::sync::Arc;
use std::time::Duration;
use std::{env, mem, thread};

mod config;
mod task;
mod queue;

trait LogError<T> {
    fn log_error(self, task_name: &str) -> Option<T>;
}

impl<T, E: std::fmt::Display> LogError<T> for Result<T, E> {
    fn log_error(self, task_name: &str) -> Option<T> {
        match self {
            Ok(ok) => Some(ok),
            Err(e) => {
                eprintln!("[{}] error: {}", task_name, e);
                None
            }
        }
    }
}

fn main() -> Result<()> {
    let arg = env::args().nth(1)
        .context("missing argument <config_file>")?;

    if matches!(arg.as_str(), "-h" | "--help" | "-help") {
        eprintln!("usage: ocron <config_file>");
        return Ok(());
    }

    // Parse config
    let mut config = Config::read_file(arg)?;

    // Print debug info
    if config.debug {
        eprintln!("{:#?}", &config);
        eprintln!();
        for (key, val) in env::vars_os() {
            eprintln!("{:?} = {:?}", key.to_string_lossy(), val.to_string_lossy());
        }
        eprintln!();
    }

    let queue = Queue::new();

    // Start tasks
    mem::take(&mut config.tasks)
        .into_iter()
        .try_for_each(|task| -> Result<()> {
            let task = Arc::new(task);
            if task.on_startup {
                let now = Local::now().naive_local();
                queue.notify_push(now, task);
            } else {
                let next = task.time.next_run()?;
                queue.notify_push(next, task);
            }
            Ok(())
        })
        .context("starting tasks")?;

    // Dispatch loop
    loop {
        while queue.wait_peek_time() < Local::now().naive_local() {
            queue.try_pop()
                .unwrap()
                .run(queue.clone());
        }

        // Ord::clamp is unstable until Rust 1.50.0
        fn clamp<T: Ord>(x: T, min: T, max: T) -> T {
            assert!(min <= max);
            Ord::min(max, Ord::max(min, x))
        }

        thread::sleep(
            // In some pathologic cases when time-traveling ocron can get stuck asleep,
            // let's limit the maximum sleep duration to 60s.
            clamp(
                (queue.wait_peek_time() - Local::now().naive_local())
                    .to_std()
                    .unwrap_or(Duration::from_secs(1)) / 2,
                Duration::from_secs(1),
                Duration::from_secs(60),
            )
        );
    }
}
