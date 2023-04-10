# MQTT Client

## Installation

> Install Rust with [rustup](https://rustup.rs) first.

Use [`cargo` command](https://crates.io) to install this tool.

```
cargo install mqtt-client-cli
```

## Usage

```
mqtt-client-cli - A CLI MQTT Client

Usage: mqtt-client-cli [OPTIONS] --host <HOST URL>

Options:
  -H, --host <HOST URL>      Host. Required
  -u, --username <USERNAME>  Username. Optional
  -p, --password <PASSWORD>  Password. Optional
  -t, --topic <TOPIC>        Topic. Optional (Default = "#")
  -h, --help                 Print help
  -V, --version              Print version
```
