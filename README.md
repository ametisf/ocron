# OCRON

OCRON is a cron implementation with an obvious configuration format instead of
the usual crontab.  The format is using TOML.


## Usage

To try it out run:

```
cargo run -- example.toml
```

or

```
cargo build && target/debug/ocron example.toml
```

The example runs `date +%H:%M:%S` each whole 10 seconds on Mondays and Fridays.

For documentation on the configuration options see
[`example.toml`](https://github.com/ametisf/ocron/blob/main/example.toml).

Note that OCRON uses a patched version of TOML which allows "inline" tables to
span multiple lines, if you install from crates.io the vanilla TOML is used
instead.
