// SPDX-License-Identifier: MPL-2.0

//! Widget implementation for the system monitor
//! This widget will read the configuration from the applet and display system information

use crate::config::Config;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::Subscription;
use cosmic::prelude::*;
use cosmic::widget;
use std::time::Duration;
use sysinfo::{System, Networks, Disks};

/// The widget model stores widget-specific state
pub struct MonitorWidget {
    /// Application state which is managed by the COSMIC runtime.
    core: cosmic::Core,
    /// Configuration data shared with the applet.
    config: Config,
    /// System monitoring data
    cpu_usage: f32,
    memory_usage: f32,
    memory_total: u64,
    memory_used: u64,
    /// Network statistics
    network_rx_bytes: u64,
    network_tx_bytes: u64,
    network_rx_rate: f64,
    network_tx_rate: f64,
    /// Disk statistics
    disk_read_rate: f64,
    disk_write_rate: f64,
    /// System information instance
    sys: System,
    networks: Networks,
    disks: Disks,
}

impl Default for MonitorWidget {
    fn default() -> Self {
        Self {
            core: Default::default(),
            config: Default::default(),
            cpu_usage: 0.0,
            memory_usage: 0.0,
            memory_total: 0,
            memory_used: 0,
            network_rx_bytes: 0,
            network_tx_bytes: 0,
            network_rx_rate: 0.0,
            network_tx_rate: 0.0,
            disk_read_rate: 0.0,
            disk_write_rate: 0.0,
            sys: System::new_all(),
            networks: Networks::new_with_refreshed_list(),
            disks: Disks::new_with_refreshed_list(),
        }
    }
}

/// Messages emitted by the widget
#[derive(Debug, Clone)]
pub enum Message {
    UpdateConfig(Config),
    UpdateSystemStats,
    Tick,
    WindowMoved(i32, i32),
}

/// Create a COSMIC application from the widget model
impl cosmic::Application for MonitorWidget {
    /// The async executor that will be used to run your application's commands.
    type Executor = cosmic::executor::Default;

    /// Data that your application receives to its init method.
    type Flags = ();

    /// Messages which the application and its widgets will emit.
    type Message = Message;

    /// Unique identifier in RDNN (reverse domain name notation) format.
    const APP_ID: &'static str = "com.github.zoliviragh.CosmicMonitorWidget";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    /// Initializes the widget with any given flags and startup commands.
    fn init(
        mut core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        let config = cosmic_config::Config::new(
            "com.github.zoliviragh.CosmicMonitor",
            Config::VERSION,
        )
        .ok()
        .and_then(|context| Config::get_entry(&context).ok())
        .unwrap_or_default();

        // Completely disable all window chrome - borderless window
        core.window.show_headerbar = false;
        core.window.use_template = false;
        core.window.content_container = false;
        core.window.sharp_corners = false;  // No rounded corners on window itself

        let widget = MonitorWidget {
            core,
            config,
            ..Default::default()
        };

        (widget, Task::none())
    }

    /// Displays the widget's interface.
    fn view(&self) -> Element<'_, Self::Message> {
        let mut content = widget::column().spacing(8).padding(12);

        // Add title
        content = content.push(widget::text::title3("System Monitor"));

        if self.config.show_cpu {
            content = content.push(
                widget::row()
                    .spacing(8)
                    .push(widget::text("CPU:").width(100))
                    .push(widget::text(format!("{:.1}%", self.cpu_usage))),
            );
        }

        if self.config.show_memory {
            if self.config.show_percentages {
                content = content.push(
                    widget::row()
                        .spacing(8)
                        .push(widget::text("Memory:").width(100))
                        .push(widget::text(format!("{:.1}%", self.memory_usage))),
                );
            } else {
                let mem_gb_used = self.memory_used as f64 / (1024.0 * 1024.0 * 1024.0);
                let mem_gb_total = self.memory_total as f64 / (1024.0 * 1024.0 * 1024.0);
                content = content.push(
                    widget::row()
                        .spacing(8)
                        .push(widget::text("Memory:").width(100))
                        .push(widget::text(format!(
                            "{:.1}/{:.1} GB",
                            mem_gb_used, mem_gb_total
                        ))),
                );
            }
        }

        if self.config.show_network {
            content = content.push(
                widget::row()
                    .spacing(8)
                    .push(widget::text("Network ↓:").width(100))
                    .push(widget::text(format!("{:.1} KB/s", self.network_rx_rate / 1024.0))),
            );
            content = content.push(
                widget::row()
                    .spacing(8)
                    .push(widget::text("Network ↑:").width(100))
                    .push(widget::text(format!("{:.1} KB/s", self.network_tx_rate / 1024.0))),
            );
        }

        if self.config.show_disk {
            content = content.push(
                widget::row()
                    .spacing(8)
                    .push(widget::text("Disk Read:").width(100))
                    .push(widget::text(format!("{:.1} KB/s", self.disk_read_rate / 1024.0))),
            );
            content = content.push(
                widget::row()
                    .spacing(8)
                    .push(widget::text("Disk Write:").width(100))
                    .push(widget::text(format!("{:.1} KB/s", self.disk_write_rate / 1024.0))),
            );
        }

        // Create a custom styled container with background and rounded corners, no border
        widget::container(content)
            .padding(8)
            .class(cosmic::theme::Container::custom(|theme| {
                widget::container::Style {
                    background: Some(cosmic::iced::Background::Color(
                        theme.cosmic().background.base.into(),
                    )),
                    border: cosmic::iced::Border {
                        radius: [8.0, 8.0, 8.0, 8.0].into(),
                        width: 0.0,  // No border
                        color: cosmic::iced::Color::TRANSPARENT,
                    },
                    ..Default::default()
                }
            }))
            .into()
    }

    /// Register subscriptions for this widget.
    fn subscription(&self) -> Subscription<Self::Message> {
        struct TickSubscription;

        Subscription::batch(vec![
            // Update timer based on config interval
            cosmic::iced::time::every(Duration::from_millis(self.config.update_interval_ms))
                .map(|_| Message::Tick),
            // Watch for configuration changes from the applet
            self.core()
                .watch_config::<Config>("com.github.zoliviragh.CosmicMonitor")
                .map(|update| Message::UpdateConfig(update.config)),
        ])
    }

    /// Handles messages emitted by the widget.
    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::UpdateConfig(config) => {
                self.config = config;
            }
            Message::Tick => {
                return Task::perform(
                    async { Message::UpdateSystemStats },
                    |msg| cosmic::Action::App(msg),
                );
            }
            Message::WindowMoved(x, y) => {
                // Only save position if widget is movable (settings is open)
                if self.config.widget_movable {
                    self.config.widget_x = x;
                    self.config.widget_y = y;
                    // Save to config
                    if let Ok(config_handler) = cosmic_config::Config::new(
                        "com.github.zoliviragh.CosmicMonitor",
                        Config::VERSION,
                    ) {
                        let _ = self.config.write_entry(&config_handler);
                    }
                }
            }
            Message::UpdateSystemStats => {
                // Update CPU usage
                self.sys.refresh_cpu_all();
                self.cpu_usage = self.sys.global_cpu_usage();

                // Update memory usage
                self.sys.refresh_memory();
                self.memory_used = self.sys.used_memory();
                self.memory_total = self.sys.total_memory();
                self.memory_usage = if self.memory_total > 0 {
                    (self.memory_used as f32 / self.memory_total as f32) * 100.0
                } else {
                    0.0
                };

                // Update network statistics
                self.networks.refresh();
                let mut total_rx = 0;
                let mut total_tx = 0;
                for (_interface_name, network) in &self.networks {
                    total_rx += network.received();
                    total_tx += network.transmitted();
                }
                
                // Calculate rates (bytes per update interval)
                let interval_secs = self.config.update_interval_ms as f64 / 1000.0;
                if self.network_rx_bytes > 0 {
                    self.network_rx_rate = (total_rx - self.network_rx_bytes) as f64 / interval_secs;
                    self.network_tx_rate = (total_tx - self.network_tx_bytes) as f64 / interval_secs;
                }
                self.network_rx_bytes = total_rx;
                self.network_tx_bytes = total_tx;

                // Update disk statistics (simplified - just getting current usage)
                self.disks.refresh();
                // For now, just show placeholder values
                // Real disk I/O rate tracking would require tracking read/write bytes over time
                self.disk_read_rate = 0.0;
                self.disk_write_rate = 0.0;
            }
        }
        Task::none()
    }

    /// Override the style to have transparent background and no borders
    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::iced_runtime::Appearance {
            background_color: cosmic::iced_core::Color::TRANSPARENT,
            text_color: cosmic::iced_core::Color::WHITE,
            icon_color: cosmic::iced_core::Color::WHITE,
        })
    }
}
