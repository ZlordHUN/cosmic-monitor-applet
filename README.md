# Cosmic Monitor Applet

System resource monitor applet and widget for COSMIC desktop environment. Provides a Conky-style monitoring solution with:

- **Applet**: Configuration interface in the COSMIC panel
- **Widget**: Standalone window displaying real-time system statistics

## Features

### Monitoring Capabilities
- **CPU Usage**: Real-time CPU utilization percentage
- **Memory Usage**: RAM usage in GB or percentage
- **Network Activity**: Download/upload rates in KB/s
- **Disk I/O**: Read/write rates (placeholder)

### Configuration Options
- Toggle individual monitors on/off
- Switch between percentage and absolute values
- Adjust update interval (100-10000ms)
- Graph visualization support (planned)

## Installation

A [justfile](./justfile) is included by default for the [casey/just][just] command runner.

- `just` builds the application with the default `just build-release` recipe
- `just run` builds and runs the applet
- `just run-widget` builds and runs the widget
- `just install` installs both the applet and widget into the system
- `just vendor` creates a vendored tarball
- `just build-vendored` compiles with vendored dependencies from that tarball
- `just check` runs clippy on the project to check for linter warnings
- `just check-json` can be used by IDEs that support LSP

## Usage

### Running the Applet
The applet integrates with the COSMIC panel:
```sh
just run
```
Click the monitor icon in the panel to open configuration options.

### Running the Widget
Launch the standalone monitoring window:
```sh
just run-widget
```

### Configuration
The applet saves configuration to `~/.config/cosmic/com.github.zoliviragh.CosmicMonitor/v1/`

Changes made in the applet are automatically picked up by the widget in real-time.

## Architecture

See [ARCHITECTURE.md](./ARCHITECTURE.md) for detailed information about the project structure.

## Translators

[Fluent][fluent] is used for localization of the software. Fluent's translation files are found in the [i18n directory](./i18n). New translations may copy the [English (en) localization](./i18n/en) of the project, rename `en` to the desired [ISO 639-1 language code][iso-codes], and then translations can be provided for each [message identifier][fluent-guide]. If no translation is necessary, the message may be omitted.

## Packaging

If packaging for a Linux distribution, vendor dependencies locally with the `vendor` rule, and build with the vendored sources using the `build-vendored` rule. When installing files, use the `rootdir` and `prefix` variables to change installation paths.

```sh
just vendor
just build-vendored
just rootdir=debian/cosmic-monitor-applet prefix=/usr install
```

It is recommended to build a source tarball with the vendored dependencies, which can typically be done by running `just vendor` on the host system before it enters the build environment.

## Developers

Developers should install [rustup][rustup] and configure their editor to use [rust-analyzer][rust-analyzer].

[fluent]: https://projectfluent.org/
[fluent-guide]: https://projectfluent.org/fluent/guide/hello.html
[iso-codes]: https://en.wikipedia.org/wiki/List_of_ISO_639-1_codes
[just]: https://github.com/casey/just
[rustup]: https://rustup.rs/
[rust-analyzer]: https://rust-analyzer.github.io/
[sccache]: https://github.com/mozilla/sccache
