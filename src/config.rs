use anyhow::{anyhow, bail, Context, Result};
use chrono::{Duration, Weekday};
use std::collections::HashMap as Map;
use std::convert::TryFrom;
use std::fmt::Debug;
use std::fs;
use std::ops::Range;
use std::path::Path;
use toml::value::{Table, Value};

#[derive(Debug)]
pub struct Config {
    pub shell: String,
    pub env: Map<String, EnvVal>,
    pub clear_env: bool,
    pub on_startup: bool,
    pub debug: bool,
    pub tasks: Vec<Task>,
}

#[derive(Debug)]
pub struct Task {
    pub name: String,
    pub command: Command,
    pub time: Time,
    pub shell: String,
    pub env: Map<String, EnvVal>,
    pub clear_env: bool,
    pub on_startup: bool,
}

#[derive(Debug)]
pub enum Command {
    Shell(String),
    Argv(Vec<String>),
}

#[derive(Clone, Debug)]
pub enum EnvVal {
    Clear,
    Set(String),
}

#[derive(Clone, Debug)]
pub enum Time {
    On {
        second: Vec<u32>,
        minute: Vec<u32>,
        hour: Vec<u32>,
        weekday: Vec<Weekday>,
        day: Vec<u32>,
        month: Vec<u32>,
    },
    Every {
        duration: Duration,
    },
    After {
        duration: Duration,
    },
}

impl Config {
    pub fn read_file<P: AsRef<Path>>(path: P) -> Result<Config> {
        let path = path.as_ref();
        let config_data = fs::read(path)
            .with_context(|| format!("cannot read config file {:?}", path))?;
        let config = toml::from_slice::<Table>(&config_data)
            .context("parsing toml")?;
        parse_config(config)
    }
}

fn parse_config(table: Table) -> Result<Config> {
    let mut config = Config {
        shell: String::from("/bin/sh"),
        env: Map::new(),
        clear_env: false,
        on_startup: false,
        debug: false,
        tasks: Vec::new(),
    };

    for (key, value) in table.into_iter() {
        match key.as_str() {
            "shell" => {
                config.shell = parse_string(value)
                    .context("parsing global `shell`")?;
            }
            "env" => {
                config.env.extend(
                    parse_env(value)
                        .context("parsing global `env`")?
                );
            }
            "clear_env" => {
                config.clear_env = parse_bool(value)
                    .context("parsing global `clear_env`")?;
            }
            "on_startup" => {
                config.on_startup = parse_bool(value)
                    .context("parsing global `on_startup`")?;
            }
            "debug" => {
                config.debug = parse_bool(value)
                    .context("parsing global `debug`")?;
            }
            "task" => {
                parse_tasks(value, &mut config)
                    .context("parsing tasks")?;
            }
            _ => bail!("unknown option `{}`, valid options are `shell`, `env`, `clear_env`, `on_startup`, \
                       `debug` and `task`.", key),
        }
    }

    Ok(config)
}

fn parse_string(value: Value) -> Result<String> {
    match value {
        Value::String(string) => Ok(string),
        _ => bail!("expected string, found `{:?}`", value),
    }
}

fn parse_table(value: Value) -> Result<Table> {
    match value {
        Value::Table(table) => Ok(table),
        _ => bail!("expected table, found `{:?}`", value),
    }
}

fn parse_bool(value: Value) -> Result<bool> {
    match value {
        Value::Boolean(b) => Ok(b),
        _ => bail!("expected bool, found `{:?}`", value),
    }
}

fn parse_integer(value: Value) -> Result<i64> {
    match value {
        Value::Integer(int) => Ok(int),
        _ => bail!("expected integer, found `{:?}`", value),
    }
}

fn parse_env(value: Value) -> Result<Map<String, EnvVal>> {
    let table = parse_table(value)?;
    table.into_iter().map(|(key, value)| {
        let val = match value {
            Value::Boolean(false) => EnvVal::Clear,
            Value::String(string) => EnvVal::Set(string),
            _ => bail!("expected string or false, found `{:?}`", value),
        };
        Ok((key, val))
    })
    .collect()
}

fn parse_tasks(value: Value, config: &mut Config) -> Result<()> {
    match value {
        Value::Array(tasks) => {
            for value in tasks.into_iter() {
                match value {
                    Value::Table(table) => {
                        if let Some(Value::String(name)) = table.get("name") {
                            let name = name.clone();
                            let task = parse_task(name.clone(), table, &config)
                                .with_context(|| format!("parsing task `{}`", name))?;
                            config.tasks.push(task);
                        } else {
                            bail!("missing `name` for task");
                        }
                    }
                    _ => bail!("task must be a table, found {:?}", value),
                }
            }
        }
        _ => bail!("option `task` must be an array, try using \"[[task]]\", found {:?}", value),
    }

    Ok(())
}

fn parse_task(name: String, table: Table, global: &Config) -> Result<Task> {
    let mut command = None;
    let mut time = None;
    let mut shell = None;
    let mut env = global.env.clone();
    let mut clear_env = global.clear_env;
    let mut on_startup = global.on_startup;

    for (key, value) in table.into_iter() {
        match key.as_str() {
            "cmd" => {
                command = Some(
                    parse_command(value)
                        .context("parsing task command (`cmd`)")?
                );
            }
            "after" | "every" | "on" => {
                if time.is_some() {
                    bail!("only one timing (options `after`, `every` and `on`) can be set");
                }
                time = Some(
                    parse_time(&key, value)
                        .with_context(|| format!("parsing task timing (`{}`)", &key))?
                );
            }
            "shell" => {
                shell = Some(
                    parse_string(value)
                        .context("parsing task `shell`")?
                );
            }
            "env" => {
                env.extend(
                    parse_env(value)
                        .context("parsing global `env`")?
                );
            }
            "clear_env" => {
                clear_env = parse_bool(value)
                    .context("parsing task `clear_env`")?;
            }
            "on_startup" => {
                on_startup = parse_bool(value)
                    .context("parsing task `on_startup`")?;
            }
            "name" => {
                // nop
            }
            _ => {
                bail!("unknown task option, valid options are `name`, `cmd`, `after`, `every`, `on`, `shell`, \
                      `clear_env` and `on_startup`");
            }
        }
    }

    let command = command.ok_or_else(|| anyhow!("missing task command, use option `cmd`"))?;
    let time = time.ok_or_else(|| anyhow!("missing task timing, use one option of `after`, `every` or `on`"))?;
    let shell = shell.unwrap_or_else(|| global.shell.clone());

    Ok(Task { name, command, time, shell, env, clear_env, on_startup })
}

fn parse_command(value: Value) -> Result<Command> {
    match value {
        Value::String(script) => {
            if script.is_empty() {
                bail!("command shell script is empty");
            }
            Ok(Command::Shell(script))
        }
        Value::Array(argv) => {
            let argv = argv.into_iter()
                .map(parse_string)
                .collect::<Result<Vec<_>>>()
                .context("parsing command argv")?;
            if argv.is_empty() {
                bail!("command argv is empty");
            }
            Ok(Command::Argv(argv))
        }
        _ => {
            bail!("expected string or array of strings, found `{:?}`", value);
        }
    }
}

fn parse_weekday(value: Value) -> Result<Weekday> {
    let string = parse_string(value)?;
    string.parse()
        .map_err(|_| anyhow!("invalid day of the week"))
}

fn parse_ranged_integer(value: Value, range: Range<i64>) -> Result<u32> {
    assert!(u32::try_from(range.start).is_ok());
    assert!(u32::try_from(range.end - 1).is_ok());

    let int = parse_integer(value)?;
    if !range.contains(&int) {
        bail!("value is out of range {}..{}", range.start, range.end);
    }
    Ok(int as u32)
}

fn parse_one_or_array_ranged(value: Value, range: Range<i64>) -> Result<Vec<u32>> {
    match value {
        Value::Integer(_) => Ok(vec![parse_ranged_integer(value, range)?]),
        Value::Array(array) => {
            let vec = array.into_iter()
                .map(|value| parse_ranged_integer(value, range.clone()))
                .collect::<Result<Vec<_>>>()?;
            if vec.is_empty() {
                bail!("array must contain at least one value, to use the default values skip the \
                       option completely");
            }
            Ok(vec)
        }
        _ => bail!("expected integer or array, found `{:?}`", value),
    }
}

fn parse_time(variant: &str, value: Value) -> Result<Time> {
    let table = parse_table(value)?;
    match variant {
        "after" | "every" => {
            let mut seconds = 0;
            let mut minutes = 0;
            let mut hours = 0;
            let mut days = 0;
            let mut weeks = 0;
            for (key, value) in table.into_iter() {
                match key.as_str() {
                    "seconds" => {
                        seconds = parse_integer(value)
                            .context("parsing option `seconds`")?;
                        if seconds < 0 { bail!("number of `seconds` must be >= 0"); }
                    }
                    "minutes" => {
                        minutes = parse_integer(value)
                            .context("parsing option `minutes`")?;
                        if minutes < 0 { bail!("number of `minutes` must be >= 0"); }
                    },
                    "hours" => {
                        hours = parse_integer(value)
                            .context("parsing option `hours`")?;
                        if hours < 0 { bail!("number of `hours` must be >= 0"); }
                    },
                    "days" => {
                        days = parse_integer(value)
                            .context("parsing option `days`")?;
                        if days < 0 { bail!("number of `days` must be >= 0"); }
                    },
                    "weeks" => {
                        weeks = parse_integer(value)
                            .context("parsing option `weeks`")?;
                        if weeks < 0 { bail!("number of `weeks` must be >= 0"); }
                    },
                    _ => bail!("unknown time option (unit) `{}`, valid units are `seconds`, `minutes`, `hours`, \
                                `days` and `weeks`", key),
                };
            }

            let duration =
                Duration::seconds(seconds) +
                Duration::minutes(minutes) +
                Duration::hours(hours) +
                Duration::days(days) +
                Duration::weeks(weeks);

            if duration < Duration::seconds(1) {
                bail!("time interval must be at least 1 second, found `{:?}`", duration);
            }

            match variant {
                "after" => Ok(Time::After { duration }),
                "every" => Ok(Time::Every { duration }),
                _ => unreachable!(),
            }
        }
        "on" => {
            let mut second = None;
            let mut minute = None;
            let mut hour = None;
            let mut day = None;
            let mut month = None;
            let mut weekday = None;
            for (key, value) in table.into_iter() {
                match key.as_str() {
                    "second" => {
                        second = Some(
                            parse_one_or_array_ranged(value, 0..60)
                                .context("parsing option `second`")?
                        );
                    }
                    "minute" => {
                        minute = Some(
                            parse_one_or_array_ranged(value, 0..60)
                                .context("parsing option `minute`")?
                        );
                    },
                    "hour" => {
                        hour = Some(
                            parse_one_or_array_ranged(value, 0..24)
                                .context("parsing option `hour`")?
                        );
                    },
                    "day" => {
                        day = Some(
                            parse_one_or_array_ranged(value, 1..32)
                                .context("parsing option `day`")?
                        );
                    },
                    "month" => {
                        month = Some(
                            parse_one_or_array_ranged(value, 1..13)
                                .context("parsing option `month`")?
                        );
                    },
                    "weekday" => {
                        weekday = Some(
                            (move || {
                                match value {
                                    Value::String(_) => Ok(vec![parse_weekday(value)?]),
                                    Value::Array(array) => {
                                        array.into_iter()
                                            .map(|value| parse_weekday(value))
                                            .collect::<Result<_>>()
                                    }
                                    _ => bail!("expected weekday or array, found `{:?}`", value),
                                }
                            })()
                            .context("parsing option `weekday`")?
                        );
                    },
                    _ => bail!("unknown time option (unit) `{}`, valid units are `second`, `minute`, `hour`, `day` \
                                `month` and `weekday`", key),
                };
            }

            if !(second.is_some() || minute.is_some() || hour.is_some() || day.is_some() ||
                    month.is_some() || weekday.is_some()) {
                bail!("at least one of `second`, `minute`, `hour`, `day`, `month` or `weekday` has to be set");
            }

            let mut second = second.unwrap_or_else(|| vec![0]);
            let mut minute = minute.unwrap_or_else(|| (0..60).collect());
            let mut hour = hour.unwrap_or_else(|| (0..24).collect());
            let mut day = day.unwrap_or_else(|| vec![]);
            let mut month = month.unwrap_or_else(|| vec![]);
            let mut weekday = weekday.unwrap_or_else(|| vec![]);

            second.sort_unstable(); second.dedup();
            minute.sort_unstable(); minute.dedup();
            hour.sort_unstable(); hour.dedup();
            day.sort_unstable(); day.dedup();
            month.sort_unstable(); month.dedup();
            weekday.sort_unstable_by_key(Weekday::number_from_monday); weekday.dedup();

            Ok(Time::On { second, minute, hour, day, month, weekday })
        }
        _ => unreachable!()
    }
}
