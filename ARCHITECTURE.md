# Cosmic Monitor Architecture

## Overview

This project consists of two main components:

1. **Applet** (`src/app.rs`, `src/main.rs`) - Configuration UI in the COSMIC panel
2. **Widget** (`src/widget.rs`) - Separate window that displays system information

## Component Structure

### Applet (Configuration UI)
- Lives in the COSMIC panel as a clickable icon
- Opens a popup window when clicked
- Provides configuration controls:
  - Toggle CPU monitoring
  - Toggle memory monitoring  
  - Toggle network monitoring
  - Toggle disk I/O monitoring
  - Toggle percentage display
  - Toggle graph display
  - Set update interval (100-10000ms)
- Configuration is saved to `~/.config/cosmic/com.github.zoliviragh.CosmicMonitor/v1/`

### Widget (Display Window)
- Separate window that shows system monitoring information
- Reads configuration from the same config file as the applet
- Automatically updates based on the configured interval
- Subscribes to config changes to update in real-time
- Currently displays placeholder data (needs system monitoring implementation)

## Configuration Flow

```
Applet UI → cosmic-config → Widget
   ↓                           ↓
Toggles/Settings    Reads config & displays data
   ↓                           ↓
Saves to file       Watches for changes
```

## Next Steps

### To Run the Applet
The applet runs as part of COSMIC panel:
```bash
just run
```

### To Create a Separate Widget Binary
You'll need to:
1. Create `src/widget_main.rs` that runs the widget
2. Update `Cargo.toml` to have multiple binaries:
```toml
[[bin]]
name = "cosmic-monitor-applet"
path = "src/main.rs"

[[bin]]
name = "cosmic-monitor-widget"
path = "src/widget_main.rs"
```

### System Monitoring Implementation
To add actual system monitoring, you'll need to add dependencies like:
- `sysinfo` for CPU, memory, disk
- `systemstat` for network statistics
- Or use `/proc` filesystem directly on Linux

Example with sysinfo:
```toml
[dependencies]
sysinfo = "0.30"
```

Then update `Message::UpdateSystemStats` in `src/widget.rs` to:
```rust
Message::UpdateSystemStats => {
    use sysinfo::{System, SystemExt, CpuExt};
    
    let mut sys = System::new_all();
    sys.refresh_all();
    
    self.cpu_usage = sys.global_cpu_info().cpu_usage();
    self.memory_used = sys.used_memory();
    self.memory_total = sys.total_memory();
    self.memory_usage = (self.memory_used as f32 / self.memory_total as f32) * 100.0;
}
```

## Files

- `src/main.rs` - Entry point for applet
- `src/app.rs` - Applet application logic
- `src/widget.rs` - Widget application logic  
- `src/config.rs` - Shared configuration structure
- `src/i18n.rs` - Localization support
- `i18n/en/cosmic_monitor_applet.ftl` - English translations
- `resources/app.desktop` - Desktop file for applet registration
