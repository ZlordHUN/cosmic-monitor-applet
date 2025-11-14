# Quick Start Guide

## Building

```bash
# Build both applet and widget
just build-release

# Or use cargo directly
cargo build --release
```

This creates two binaries:
- `target/release/cosmic-monitor-applet` - Panel applet for configuration
- `target/release/cosmic-monitor-widget` - Standalone monitoring window

## Running

### Applet (Configuration UI)
```bash
just run
# Or: cargo run --release --bin cosmic-monitor-applet
```

The applet will appear as an icon in your COSMIC panel. Click it to configure:
- Which metrics to monitor (CPU, Memory, Network, Disk)
- Display format (percentages vs absolute values)
- Update interval in milliseconds

### Widget (Display Window)
```bash
just run-widget
# Or: cargo run --release --bin cosmic-monitor-widget
```

The widget shows live system statistics based on your applet configuration.

## Configuration

All settings are stored in:
```
~/.config/cosmic/com.github.zoliviragh.CosmicMonitor/v1/
```

The widget automatically detects configuration changes from the applet.

## Installing

```bash
# Build and install to /usr
just build-release
sudo just install

# Install to custom location
sudo just prefix=/usr/local install
```

This installs:
- `/usr/bin/cosmic-monitor-applet`
- `/usr/bin/cosmic-monitor-widget`
- `/usr/share/applications/com.github.zoliviragh.CosmicMonitor.desktop`
- `/usr/share/applications/com.github.zoliviragh.CosmicMonitor.Widget.desktop`

## Tips

### Auto-start Widget
Add `cosmic-monitor-widget` to your COSMIC startup applications.

### Keyboard Shortcut
Bind a keyboard shortcut to launch `cosmic-monitor-widget` for quick access.

### Multiple Widgets
You can run multiple widget instances if you want different views.

### Network Monitoring
Network rates are calculated per update interval. For smoother readings, use shorter intervals (500-1000ms).

## Troubleshooting

### Widget shows 0% CPU
Make sure to wait one update interval for the first reading.

### Configuration not updating
The widget watches for config file changes. If it doesn't update, try restarting the widget.

### Permission errors
Some system information may require elevated privileges on certain systems.
