use crate::config::{Command, EnvVal, Task, Time};
use crate::queue::Queue;
use crate::LogError;
use anyhow::{bail, Result};
use chrono::prelude::*;
use chrono::Duration;
use std::process::Command as Subprocess;
use std::sync::Arc;
use std::thread;

impl Task {
    pub fn run(self: Arc<Self>, queue: Arc<Queue>) {
        let mut command = match &self.command {
            Command::Shell(script) => {
                let mut c = Subprocess::new(&self.shell);
                c.arg("-c").arg(script);
                c
            }
            Command::Argv(args) => {
                if args.is_empty() { unreachable!("checked when parsing config"); }
                let mut c = Subprocess::new(&args[0]);
                c.args(&args[1..]);
                c
            }
        };

        if self.clear_env {
            command.env_clear();
        }
        self.env.iter().for_each(|(key, val)| match val {
            EnvVal::Set(val) => {
                command.env(key, val);
            }
            EnvVal::Clear => {
                command.env_remove(key);
            }
        });

        thread::spawn(move || {
            eprintln!("[{}] running: {:?}", self.name, command);

            if let Time::On { .. } | Time::Every { .. } = &self.time {
                self.time.next_run()
                    .log_error(&self.name)
                    .map(|next| queue.notify_push(next, self.clone()));
            }

            command.spawn()
                .log_error(&self.name)
                .map(|mut child| {
                    child.wait()
                        .log_error(&self.name)
                        .map(|status| eprintln!("[{}] finished: {}", &self.name, status));
                });

            if let Time::After { .. } = &self.time {
                self.time.next_run()
                    .log_error(&self.name)
                    .map(|next| queue.notify_push(next, self));
            }
        });
    }
}

impl Time {
    pub fn next_run(&self) -> Result<NaiveDateTime> {
        let now = Local::now().naive_local();
        match self {
            Time::After { duration } |
            Time::Every { duration } => {
                Ok(now + *duration)
            }
            Time::On { second, minute, hour, weekday, day, month } => {
                find_next_datetime(now, second, minute, hour, weekday, day, month)
            }
        }
    }
}

// Performs a linear search for the next viable DateTime.
//
// First searches through every combination of hour, minute and second in lexicographic order
// and checks if they fit in the current date, otherwise chooses the first combination and skips
// one day.
//
// Then it checks each day whether it matches the criteria in the next LOOKAHEAD days.  This may
// sound expensive and unncecessary, but the bounds are still low enough that this function
// finishes fast enough even in the worst case scenario (the benchmark finishes <1ms).
//
// The benefit of this naive method is that the function is easy to understand which beats a minor
// inefficiency any day.
fn find_next_datetime(
    now: NaiveDateTime,
    second: &[u32],
    minute: &[u32],
    hour: &[u32],
    weekday: &[Weekday],
    day: &[u32],
    month: &[u32],
) -> Result<NaiveDateTime> {
    // Find time
    let mut date = now.date();
    let now = now.time();

    let mut time = None;
    'outer: for &h in hour {
        for &m in minute {
            for &s in second {
                let t = NaiveTime::from_hms(h, m, s);
                if t > now {
                    time = Some(t);
                    break 'outer;
                }
            }
        }
    }

    let time = time.unwrap_or_else(|| {
        date += Duration::days(1);
        NaiveTime::from_hms(hour[0], minute[0], second[0])
    });

    const LOOKAHEAD: usize = 366 * 4 * 7;

    // Find date
    for _ in 0..LOOKAHEAD {
        if (weekday.is_empty() || weekday.contains(&date.weekday()))
        && (day.is_empty() || day.contains(&date.day()))
        && (month.is_empty() || month.contains(&date.month())) {
            return Ok(NaiveDateTime::new(date, time));
        }

        date += Duration::days(1);
    }

    bail!("didn't find a date the next {} days", LOOKAHEAD)
}

#[cfg(test)]
extern crate test;

#[cfg(test)]
#[bench]
fn worst_case_search(b: &mut test::Bencher) {
    use std::hint::black_box;

    b.iter(|| {
        let now = NaiveDateTime::new(
            NaiveDate::from_ymd(2020, 12, 04),
            NaiveTime::from_hms(23, 59, 59),
        );
        // The worst case is achieved by giving it fake input.  Criteria which force the most
        // checks are done but match late or never.  Input like this should never pass the parser,
        // but we are testing the absolute worst case.
        let out = find_next_datetime(
            black_box(now),
            black_box(&(0..60).collect::<Vec<u32>>()),
            black_box(&(0..60).collect::<Vec<u32>>()),
            black_box(&(0..24).collect::<Vec<u32>>()),
            black_box(&[Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri, Weekday::Sat, Weekday::Sun]),
            black_box(&(0..50).collect::<Vec<u32>>()),
            black_box(&(13..30).collect::<Vec<u32>>()),
        );
        let _ = black_box(out);
    })
}
