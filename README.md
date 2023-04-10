# MQTT Client

A CLI tool for fetching MQTT stream data.

- Output can be printed out directly, or directly written into a text file with
  the `-o` option.

- Includes an HTTP Server (http://0.0.0.0:12345/) for fetching all available
  data with JSON.

## Installation

1. Install Rust with [rustup](https://rustup.rs) (Skip this step if already
   installed.)

2. Install dependencies (CMake and build related tools) from the package
   manager:

   - macOS: `brew install cmake`
   - Debian-based distro: `sudo apt install build-essential cmake`

3. Use [`cargo` command](https://crates.io) to install this tool.

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
  -o, --output <OUTPUT>      Output Path. All data from the MQTT stream will be stored into the specified file
  -h, --help                 Print help
  -V, --version              Print version
```
