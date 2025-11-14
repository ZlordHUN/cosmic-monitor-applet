// SPDX-License-Identifier: MPL-2.0

//! Widget implementation using Wayland layer-shell protocol
//! This bypasses the compositor's window management to achieve borderless rendering

mod config;
mod widget;

use config::Config;
use widget::{UtilizationMonitor, TemperatureMonitor, NetworkMonitor, WeatherMonitor};
use widget::utilization::{draw_cpu_icon, draw_ram_icon, draw_gpu_icon, draw_progress_bar};
use widget::temperature::draw_temp_circle;
use widget::weather::draw_weather_icon;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use std::sync::Arc;
use std::time::Instant;

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_shm,
    delegate_seat, delegate_pointer,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{Capability, SeatHandler, SeatState},
    seat::pointer::{PointerHandler, PointerEvent, PointerEventKind},
    shell::{
        wlr_layer::{
            Anchor, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_output, wl_shm, wl_surface},
    Connection, QueueHandle,
};

const WIDGET_WIDTH: u32 = 350;
const WIDGET_HEIGHT: u32 = 400;

struct MonitorWidget {
    registry_state: RegistryState,
    output_state: OutputState,
    compositor_state: CompositorState,
    shm_state: Shm,
    layer_shell: LayerShell,
    seat_state: SeatState,
    
    /// The main surface for rendering
    layer_surface: Option<LayerSurface>,
    
    /// Configuration
    config: Arc<Config>,
    config_handler: cosmic_config::Config,
    last_config_check: Instant,
    
    /// System monitoring modules
    utilization: UtilizationMonitor,
    temperature: TemperatureMonitor,
    network: NetworkMonitor,
    weather: WeatherMonitor,
    last_update: Instant,
    
    /// Memory pool for rendering
    pool: Option<SlotPool>,
    
    /// Track last widget height for resizing
    last_height: u32,
    
    /// Mouse dragging state
    dragging: bool,
    drag_start_x: f64,
    drag_start_y: f64,
    
    /// Exit flag
    exit: bool,
}

impl CompositorHandler for MonitorWidget {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
        // Handle scale factor changes if needed
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
        // Handle transform changes if needed
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        self.draw(qh);
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for MonitorWidget {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for MonitorWidget {
    fn closed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
    ) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        if configure.new_size.0 == 0 || configure.new_size.1 == 0 {
            // Use our default size
        }
        self.draw(qh);
    }
}

impl SeatHandler for MonitorWidget {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wayland_client::protocol::wl_seat::WlSeat) {}
    fn new_capability(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, seat: wayland_client::protocol::wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Pointer {
            // Request pointer events
            let _ = self.seat_state.get_pointer(qh, &seat);
        }
    }
    fn remove_capability(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wayland_client::protocol::wl_seat::WlSeat, _capability: Capability) {}
    fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wayland_client::protocol::wl_seat::WlSeat) {}
}

impl PointerHandler for MonitorWidget {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wayland_client::protocol::wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        // Layer-shell surfaces in COSMIC can't be interactively moved by users
        // Position is controlled via config file (widget_x, widget_y)
        // This handler is here for potential future use
        if !self.config.widget_movable {
            return;
        }

        for event in events {
            match event.kind {
                PointerEventKind::Press { button, .. } if button == 0x110 => {
                    self.dragging = true;
                    self.drag_start_x = event.position.0;
                    self.drag_start_y = event.position.1;
                }
                PointerEventKind::Release { button, .. } if button == 0x110 => {
                    self.dragging = false;
                }
                PointerEventKind::Motion { .. } if self.dragging => {
                    let delta_x = (event.position.0 - self.drag_start_x) as i32;
                    let delta_y = (event.position.1 - self.drag_start_y) as i32;
                    
                    let mut new_config = (*self.config).clone();
                    new_config.widget_x += delta_x;
                    new_config.widget_y += delta_y;
                    
                    if new_config.write_entry(&self.config_handler).is_ok() {
                        self.config = Arc::new(new_config);
                        
                        if let Some(layer_surface) = &self.layer_surface {
                            layer_surface.set_margin(self.config.widget_y, 0, 0, self.config.widget_x);
                            layer_surface.commit();
                        }
                    }
                    
                    self.drag_start_x = event.position.0;
                    self.drag_start_y = event.position.1;
                }
                _ => {}
            }
        }
    }
}

impl ShmHandler for MonitorWidget {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm_state
    }
}

impl MonitorWidget {
    fn new(
        globals: &wayland_client::globals::GlobalList,
        qh: &QueueHandle<Self>,
        config: Config,
        config_handler: cosmic_config::Config,
    ) -> Self {
        let registry_state = RegistryState::new(globals);
        let output_state = OutputState::new(globals, qh);
        let compositor_state = CompositorState::bind(globals, qh)
            .expect("wl_compositor not available");
        let shm_state = Shm::bind(globals, qh).expect("wl_shm not available");
        let layer_shell = LayerShell::bind(globals, qh).expect("layer shell not available");
        let seat_state = SeatState::new(globals, qh);

        // Clone weather config values before moving config
        let weather_api_key = config.weather_api_key.clone();
        let weather_location = config.weather_location.clone();

        Self {
            registry_state,
            output_state,
            compositor_state,
            shm_state,
            layer_shell,
            seat_state,
            layer_surface: None,
            config: Arc::new(config),
            config_handler,
            last_config_check: Instant::now(),
            utilization: UtilizationMonitor::new(),
            temperature: TemperatureMonitor::new(),
            network: NetworkMonitor::new(),
            weather: WeatherMonitor::new(weather_api_key, weather_location),
            last_update: Instant::now(),
            pool: None,
            last_height: WIDGET_HEIGHT,
            dragging: false,
            drag_start_x: 0.0,
            drag_start_y: 0.0,
            exit: false,
        }
    }

    fn create_layer_surface(&mut self, qh: &QueueHandle<Self>) {
        let surface = self.compositor_state.create_surface(qh);
        
        let layer_surface = self.layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Top,  // Use Top layer for better interaction
            Some("cosmic-monitor-widget"),
            None,
        );

        // Configure the layer surface
        layer_surface.set_anchor(Anchor::TOP | Anchor::LEFT); // Anchor to top-left corner
        layer_surface.set_size(WIDGET_WIDTH, WIDGET_HEIGHT);
        layer_surface.set_exclusive_zone(-1); // Don't reserve space
        eprintln!("Setting layer surface margins: top={}, left={}", self.config.widget_y, self.config.widget_x);
        layer_surface.set_margin(self.config.widget_y, 0, 0, self.config.widget_x);
        layer_surface.set_keyboard_interactivity(
            smithay_client_toolkit::shell::wlr_layer::KeyboardInteractivity::None
        );
        
        layer_surface.commit();
        
        self.layer_surface = Some(layer_surface);
    }

    fn update_system_stats(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        
        if elapsed < (self.config.update_interval_ms as f64 / 1000.0) {
            return;
        }
        
        self.last_update = now;

        // Update monitoring modules
        self.utilization.update();
        self.temperature.update();
        self.network.update();
        
        // Update weather (has its own rate limiting - every 10 minutes)
        if self.config.show_weather {
            self.weather.update();
        }
    }

    fn draw(&mut self, _qh: &QueueHandle<Self>) {
        let layer_surface = match &self.layer_surface {
            Some(ls) => ls.clone(),
            None => return,
        };

        self.update_system_stats();
        
        // Calculate dynamic height based on enabled components
        let mut required_height = 10; // Base padding
        
        // Clock and date
        if self.config.show_clock {
            required_height += 70; // Clock height
        }
        if self.config.show_date {
            required_height += 35; // Date height
        }
        if self.config.show_clock || self.config.show_date {
            required_height += 20; // Spacing after clock/date
        }
        
        // Utilization section
        if self.config.show_cpu || self.config.show_memory || self.config.show_gpu {
            required_height += 35; // "Utilization" header (increased to 35)
            if self.config.show_cpu {
                required_height += 30; // CPU bar
            }
            if self.config.show_memory {
                required_height += 30; // RAM bar
            }
            if self.config.show_gpu {
                required_height += 30; // GPU bar
            }
        }
        
        // Temperature section
        if self.config.show_cpu_temp || self.config.show_gpu_temp {
            required_height += 10; // Spacing before temps
            required_height += 35; // "Temperatures" header (increased to 35)
            
            if self.config.use_circular_temp_display {
                // Circular display: larger height for circles
                required_height += 60; // Circular temp display height
            } else {
                // Text display
                if self.config.show_cpu_temp {
                    required_height += 25; // CPU temp
                }
                if self.config.show_gpu_temp {
                    required_height += 25; // GPU temp
                }
            }
        }
        
        // Network section
        if self.config.show_network {
            required_height += 50; // Two network lines
        }
        
        // Disk section
        if self.config.show_disk {
            required_height += 50; // Two disk lines
        }
        
        // Weather section
        if self.config.show_weather {
            required_height += 10; // Spacing before header
            required_height += 35; // Header
            required_height += 70; // Icon and text content (increased for bottom text clearance)
        }
        
        required_height += 20; // Bottom padding
        
        let width = WIDGET_WIDTH as i32;
        let height = required_height.max(100) as i32; // Minimum 100px height
        let stride = width * 4;

        // Update layer surface size if height changed OR create pool if it doesn't exist
        if height as u32 != self.last_height || self.pool.is_none() {
            self.last_height = height as u32;
            layer_surface.set_size(width as u32, height as u32);
            layer_surface.commit();
            
            // Recreate pool with new size
            self.pool = Some(SlotPool::new(width as usize * height as usize * 4, &self.shm_state)
                .expect("Failed to create pool"));
        }

        // Store the data we need for rendering
        let cpu_usage = self.utilization.cpu_usage;
        let memory_usage = self.utilization.memory_usage;
        let memory_used = self.utilization.memory_used;
        let memory_total = self.utilization.memory_total;
        let cpu_temp = self.temperature.cpu_temp;
        let gpu_temp = self.temperature.gpu_temp;
        let network_rx_rate = self.network.network_rx_rate;
        let network_tx_rate = self.network.network_tx_rate;
        let show_cpu = self.config.show_cpu;
        let show_memory = self.config.show_memory;
        let show_network = self.config.show_network;
        let show_disk = self.config.show_disk;
        let show_gpu = self.config.show_gpu;
        let show_cpu_temp = self.config.show_cpu_temp;
        let show_gpu_temp = self.config.show_gpu_temp;
        let show_clock = self.config.show_clock;
        let show_date = self.config.show_date;
        let show_percentages = self.config.show_percentages;
        let use_24hour_time = self.config.use_24hour_time;
        let use_circular_temp_display = self.config.use_circular_temp_display;
        let show_weather = self.config.show_weather;
        
        // Extract weather data
        let (weather_temp, weather_desc, weather_location, weather_icon) = if let Some(ref data) = self.weather.weather_data {
            (data.temperature, data.description.as_str(), data.location.as_str(), data.icon.as_str())
        } else {
            (0.0, "No data", "Unknown", "01d")
        };

        let pool = self.pool.as_mut().unwrap();

        let (buffer, canvas) = pool
            .create_buffer(width, height, stride, wl_shm::Format::Argb8888)
            .expect("Failed to create buffer");

        // Use Cairo for rendering
        render_widget(
            canvas,
            width,
            height,
            cpu_usage,
            memory_usage,
            memory_used,
            memory_total,
            cpu_temp,
            gpu_temp,
            network_rx_rate,
            network_tx_rate,
            show_cpu,
            show_memory,
            show_network,
            show_disk,
            show_gpu,
            show_cpu_temp,
            show_gpu_temp,
            show_clock,
            show_date,
            show_percentages,
            use_24hour_time,
            use_circular_temp_display,
            show_weather,
            weather_temp,
            weather_desc,
            weather_location,
            weather_icon,
        );

        // Attach the buffer to the surface
        layer_surface
            .wl_surface()
            .attach(Some(buffer.wl_buffer()), 0, 0);
        layer_surface.wl_surface().damage_buffer(0, 0, width, height);
        
        // Commit changes
        layer_surface.wl_surface().commit();
    }
}

fn render_widget(
    canvas: &mut [u8],
    width: i32,
    height: i32,
    cpu_usage: f32,
    memory_usage: f32,
    _memory_used: u64,
    _memory_total: u64,
    cpu_temp: f32,
    gpu_temp: f32,
    network_rx_rate: f64,
    network_tx_rate: f64,
    show_cpu: bool,
    show_memory: bool,
    show_network: bool,
    show_disk: bool,
    show_gpu: bool,
    show_cpu_temp: bool,
    show_gpu_temp: bool,
    show_clock: bool,
    show_date: bool,
    show_percentages: bool,
    use_24hour_time: bool,
    use_circular_temp_display: bool,
    show_weather: bool,
    weather_temp: f32,
    weather_desc: &str,
    weather_location: &str,
    weather_icon: &str,
) {
    // Use unsafe to extend the lifetime for Cairo
    // This is safe because the surface doesn't outlive the canvas buffer
    let surface = unsafe {
        let ptr = canvas.as_mut_ptr();
        let len = canvas.len();
        let static_slice: &'static mut [u8] = std::slice::from_raw_parts_mut(ptr, len);
        
        cairo::ImageSurface::create_for_data(
            static_slice,
            cairo::Format::ARgb32,
            width,
            height,
            width * 4,
        )
        .expect("Failed to create cairo surface")
    };

    {
        let cr = cairo::Context::new(&surface).expect("Failed to create cairo context");

        // Clear background to fully transparent
        cr.save().expect("Failed to save");
        cr.set_operator(cairo::Operator::Source);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
        cr.paint().expect("Failed to clear");
        cr.restore().expect("Failed to restore");

        // Set up Pango for text rendering
        let layout = pangocairo::functions::create_layout(&cr);
        
        // Track vertical position
        let mut y_pos = 10.0;
        
        // Get current date/time
        let now = chrono::Local::now();
        
        if show_clock {
            // Draw large time (HH:MM or h:MM based on format)
            let time_str = if use_24hour_time {
                now.format("%H:%M").to_string()
            } else {
                now.format("%-I:%M").to_string()
            };
            let font_desc = pango::FontDescription::from_string("Ubuntu Bold 48");
            layout.set_font_description(Some(&font_desc));
            layout.set_text(&time_str);
            
            // White text with black outline
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.move_to(10.0, y_pos);
            
            // Draw outline
            cr.set_line_width(3.0);
            pangocairo::functions::layout_path(&cr, &layout);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            cr.stroke_preserve().expect("Failed to stroke");
            
            // Fill with white
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.fill().expect("Failed to fill");
            
            // Get width of the time text to position seconds correctly
            let (time_width, _) = layout.pixel_size();
            
            // Draw seconds (:SS) slightly smaller and raised
            let seconds_str = now.format(":%S").to_string();
            let font_desc = pango::FontDescription::from_string("Ubuntu Bold 28");
            layout.set_font_description(Some(&font_desc));
            layout.set_text(&seconds_str);
            
            cr.move_to(10.0 + time_width as f64, y_pos + 5.0); // Position after HH:MM, slightly lower
            pangocairo::functions::layout_path(&cr, &layout);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            cr.stroke_preserve().expect("Failed to stroke");
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.fill().expect("Failed to fill");
            
            // For 12-hour format, add AM/PM indicator
            if !use_24hour_time {
                let ampm_str = now.format(" %p").to_string();
                let font_desc = pango::FontDescription::from_string("Ubuntu Bold 20");
                layout.set_font_description(Some(&font_desc));
                layout.set_text(&ampm_str);
                
                let (seconds_width, _) = layout.pixel_size();
                cr.move_to(10.0 + time_width as f64 + seconds_width as f64, y_pos + 10.0);
                pangocairo::functions::layout_path(&cr, &layout);
                cr.set_source_rgb(0.0, 0.0, 0.0);
                cr.stroke_preserve().expect("Failed to stroke");
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.fill().expect("Failed to fill");
            }
            
            y_pos += 70.0; // Move down after clock
        }
        
        if show_date {
            // Draw date below with more spacing
            let date_str = now.format("%A, %d %B %Y").to_string();
            let font_desc = pango::FontDescription::from_string("Ubuntu 16");
            layout.set_font_description(Some(&font_desc));
            layout.set_text(&date_str);
            
            cr.move_to(10.0, y_pos);
            pangocairo::functions::layout_path(&cr, &layout);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            cr.stroke_preserve().expect("Failed to stroke");
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.fill().expect("Failed to fill");
            
            y_pos += 35.0; // Move down after date
        }
        
        // Add spacing before stats if we showed clock or date
        if show_clock || show_date {
            y_pos += 20.0;
        } else {
            y_pos = 10.0; // Start at top if no clock/date
        }
        
        // Start system stats
        let mut y = y_pos;
        let icon_size = 20.0;
        let bar_width = 200.0;
        let bar_height = 12.0;

        // Draw stats with outline effect
        let font_desc = pango::FontDescription::from_string("Ubuntu 12");
        layout.set_font_description(Some(&font_desc));
        cr.set_line_width(2.0);
        
        // Draw "Utilization" header if any utilization metrics are shown
        if show_cpu || show_memory || show_gpu {
            let header_font = pango::FontDescription::from_string("Ubuntu Bold 14");
            layout.set_font_description(Some(&header_font));
            layout.set_text("Utilization");
            cr.move_to(10.0, y);
            pangocairo::functions::layout_path(&cr, &layout);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            cr.stroke_preserve().expect("Failed to stroke");
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.fill().expect("Failed to fill");
            
            y += 35.0; // Increased to 35px for more spacing
            
            // Reset to normal font
            let font_desc = pango::FontDescription::from_string("Ubuntu 12");
            layout.set_font_description(Some(&font_desc));
        }
        
        if show_cpu {
            // Draw CPU icon
            draw_cpu_icon(&cr, 10.0, y - 2.0, icon_size);
            
            // Draw CPU label
            layout.set_text("CPU:");
            cr.move_to(10.0 + icon_size + 10.0, y);
            pangocairo::functions::layout_path(&cr, &layout);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            cr.stroke_preserve().expect("Failed to stroke");
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.fill().expect("Failed to fill");
            
            // Draw progress bar
            draw_progress_bar(&cr, 90.0, y, bar_width, bar_height, cpu_usage);
            
            // Draw CPU percentage only if show_percentages is enabled
            if show_percentages {
                let cpu_text = format!("{:.1}%", cpu_usage);
                layout.set_text(&cpu_text);
                cr.move_to(300.0, y);
                pangocairo::functions::layout_path(&cr, &layout);
                cr.set_source_rgb(0.0, 0.0, 0.0);
                cr.stroke_preserve().expect("Failed to stroke");
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.fill().expect("Failed to fill");
            }
            
            y += 30.0;
        }

        if show_memory {
            // Draw RAM icon
            draw_ram_icon(&cr, 10.0, y - 2.0, icon_size);
            
            // Draw Memory label
            layout.set_text("RAM:");
            cr.move_to(10.0 + icon_size + 10.0, y);
            pangocairo::functions::layout_path(&cr, &layout);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            cr.stroke_preserve().expect("Failed to stroke");
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.fill().expect("Failed to fill");
            
            // Draw progress bar first
            draw_progress_bar(&cr, 90.0, y, bar_width, bar_height, memory_usage);
            
            // Draw memory percentage only if show_percentages is enabled
            if show_percentages {
                let mem_text = format!("{:.1}%", memory_usage);
                layout.set_text(&mem_text);
                cr.move_to(300.0, y); // Position after the bar
                pangocairo::functions::layout_path(&cr, &layout);
                cr.set_source_rgb(0.0, 0.0, 0.0);
                cr.stroke_preserve().expect("Failed to stroke");
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.fill().expect("Failed to fill");
            }
            
            y += 30.0;
        }

        if show_gpu {
            // Draw GPU icon
            draw_gpu_icon(&cr, 10.0, y - 2.0, icon_size);
            
            // Draw GPU label
            layout.set_text("GPU:");
            cr.move_to(10.0 + icon_size + 10.0, y);
            pangocairo::functions::layout_path(&cr, &layout);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            cr.stroke_preserve().expect("Failed to stroke");
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.fill().expect("Failed to fill");
            
            // Draw progress bar
            let gpu_usage = 0.0; // TODO: Implement actual GPU monitoring
            draw_progress_bar(&cr, 90.0, y, bar_width, bar_height, gpu_usage);
            
            // Draw GPU percentage only if show_percentages is enabled (placeholder - needs nvtop/radeontop integration)
            if show_percentages {
                let gpu_text = format!("{:.1}%", gpu_usage);
                layout.set_text(&gpu_text);
                cr.move_to(300.0, y);
                pangocairo::functions::layout_path(&cr, &layout);
                cr.set_source_rgb(0.0, 0.0, 0.0);
                cr.stroke_preserve().expect("Failed to stroke");
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.fill().expect("Failed to fill");
            }
            
            y += 30.0;
        }

        // Temperature section - show if either CPU or GPU temp is enabled
        if show_cpu_temp || show_gpu_temp {
            // Add spacing before temperature section
            y += 10.0;
            
            // Draw temperature section label
            let font_desc = pango::FontDescription::from_string("Ubuntu Bold 14");
            layout.set_font_description(Some(&font_desc));
            layout.set_text("Temperatures");
            cr.move_to(10.0, y);
            pangocairo::functions::layout_path(&cr, &layout);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            cr.stroke_preserve().expect("Failed to stroke");
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.fill().expect("Failed to fill");
            y += 35.0; // Increased to 35px for more spacing

            if use_circular_temp_display {
                // Circular temperature display
                let circle_radius = 25.0;
                let circle_diameter = circle_radius * 2.0;
                let spacing = 20.0;
                let mut x_offset = 15.0;
                
                // Maximum temperature for scaling (100°C)
                let max_temp = 100.0;
                
                // CPU Temperature Circle
                if show_cpu_temp {
                    draw_temp_circle(&cr, x_offset, y, circle_radius, cpu_temp, max_temp);
                    
                    // Draw temperature value in center
                    let temp_text = if cpu_temp > 0.0 {
                        format!("{:.0}°", cpu_temp)
                    } else {
                        "N/A".to_string()
                    };
                    let font_desc = pango::FontDescription::from_string("Ubuntu Bold 12");
                    layout.set_font_description(Some(&font_desc));
                    layout.set_text(&temp_text);
                    let (text_width, text_height) = layout.pixel_size();
                    cr.move_to(
                        x_offset + circle_radius - text_width as f64 / 2.0,
                        y + circle_radius - text_height as f64 / 2.0
                    );
                    pangocairo::functions::layout_path(&cr, &layout);
                    cr.set_source_rgb(0.0, 0.0, 0.0);
                    cr.stroke_preserve().expect("Failed to stroke");
                    cr.set_source_rgb(1.0, 1.0, 1.0);
                    cr.fill().expect("Failed to fill");
                    
                    // Draw "CPU" label below circle
                    let label_font = pango::FontDescription::from_string("Ubuntu 10");
                    layout.set_font_description(Some(&label_font));
                    layout.set_text("CPU");
                    let (label_width, _) = layout.pixel_size();
                    cr.move_to(
                        x_offset + circle_radius - label_width as f64 / 2.0,
                        y + circle_diameter + 2.0
                    );
                    pangocairo::functions::layout_path(&cr, &layout);
                    cr.set_source_rgb(0.0, 0.0, 0.0);
                    cr.stroke_preserve().expect("Failed to stroke");
                    cr.set_source_rgb(1.0, 1.0, 1.0);
                    cr.fill().expect("Failed to fill");
                    
                    x_offset += circle_diameter + spacing;
                }
                
                // GPU Temperature Circle
                if show_gpu_temp {
                    draw_temp_circle(&cr, x_offset, y, circle_radius, gpu_temp, max_temp);
                    
                    // Draw temperature value in center
                    let temp_text = if gpu_temp > 0.0 {
                        format!("{:.0}°", gpu_temp)
                    } else {
                        "N/A".to_string()
                    };
                    let font_desc = pango::FontDescription::from_string("Ubuntu Bold 12");
                    layout.set_font_description(Some(&font_desc));
                    layout.set_text(&temp_text);
                    let (text_width, text_height) = layout.pixel_size();
                    cr.move_to(
                        x_offset + circle_radius - text_width as f64 / 2.0,
                        y + circle_radius - text_height as f64 / 2.0
                    );
                    pangocairo::functions::layout_path(&cr, &layout);
                    cr.set_source_rgb(0.0, 0.0, 0.0);
                    cr.stroke_preserve().expect("Failed to stroke");
                    cr.set_source_rgb(1.0, 1.0, 1.0);
                    cr.fill().expect("Failed to fill");
                    
                    // Draw "GPU" label below circle
                    let label_font = pango::FontDescription::from_string("Ubuntu 10");
                    layout.set_font_description(Some(&label_font));
                    layout.set_text("GPU");
                    let (label_width, _) = layout.pixel_size();
                    cr.move_to(
                        x_offset + circle_radius - label_width as f64 / 2.0,
                        y + circle_diameter + 2.0
                    );
                    pangocairo::functions::layout_path(&cr, &layout);
                    cr.set_source_rgb(0.0, 0.0, 0.0);
                    cr.stroke_preserve().expect("Failed to stroke");
                    cr.set_source_rgb(1.0, 1.0, 1.0);
                    cr.fill().expect("Failed to fill");
                }
                
                y += circle_diameter + 15.0; // Move down past circles and labels
            } else {
                // Text temperature display
                let font_desc = pango::FontDescription::from_string("Ubuntu 14");
                layout.set_font_description(Some(&font_desc));

                // CPU Temperature
                if show_cpu_temp {
                    if cpu_temp > 0.0 {
                        layout.set_text(&format!("  CPU: {:.1}°C", cpu_temp));
                    } else {
                        layout.set_text("  CPU: N/A");
                    }
                    cr.move_to(10.0, y);
                    pangocairo::functions::layout_path(&cr, &layout);
                    cr.set_source_rgb(0.0, 0.0, 0.0);
                    cr.stroke_preserve().expect("Failed to stroke");
                    cr.set_source_rgb(1.0, 1.0, 1.0);
                    cr.fill().expect("Failed to fill");
                    y += 25.0;
                }

                // GPU Temperature
                if show_gpu_temp {
                    if gpu_temp > 0.0 {
                        layout.set_text(&format!("  GPU: {:.1}°C", gpu_temp));
                    } else {
                        layout.set_text("  GPU: N/A");
                    }
                    cr.move_to(10.0, y);
                    pangocairo::functions::layout_path(&cr, &layout);
                    cr.set_source_rgb(0.0, 0.0, 0.0);
                    cr.stroke_preserve().expect("Failed to stroke");
                    cr.set_source_rgb(1.0, 1.0, 1.0);
                    cr.fill().expect("Failed to fill");
                    y += 25.0;
                }
            }
        }

        if show_network {
            layout.set_text(&format!("Network ↓: {:.1} KB/s", network_rx_rate / 1024.0));
            cr.move_to(10.0, y);
            pangocairo::functions::layout_path(&cr, &layout);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            cr.stroke_preserve().expect("Failed to stroke");
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.fill().expect("Failed to fill");
            y += 25.0;

            layout.set_text(&format!("Network ↑: {:.1} KB/s", network_tx_rate / 1024.0));
            cr.move_to(10.0, y);
            pangocairo::functions::layout_path(&cr, &layout);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            cr.stroke_preserve().expect("Failed to stroke");
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.fill().expect("Failed to fill");
            y += 25.0;
        }

        if show_disk {
            layout.set_text("Disk Read: 0.0 KB/s");
            cr.move_to(10.0, y);
            pangocairo::functions::layout_path(&cr, &layout);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            cr.stroke_preserve().expect("Failed to stroke");
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.fill().expect("Failed to fill");
            y += 25.0;

            layout.set_text("Disk Write: 0.0 KB/s");
            cr.move_to(10.0, y);
            pangocairo::functions::layout_path(&cr, &layout);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            cr.stroke_preserve().expect("Failed to stroke");
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.fill().expect("Failed to fill");
            y += 25.0;
        }
        
        // Weather section
        if show_weather {
            // Add spacing before weather section
            y += 10.0;
            
            // Section header
            let header_font = pango::FontDescription::from_string("Ubuntu Bold 14");
            layout.set_font_description(Some(&header_font));
            layout.set_text("Weather");
            cr.move_to(10.0, y);
            pangocairo::functions::layout_path(&cr, &layout);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            cr.set_line_width(2.0);
            cr.stroke_preserve().expect("Failed to stroke");
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.fill().expect("Failed to fill");
            y += 35.0; // Increased to 35px for more spacing
            
            // Draw weather icon
            let icon_size = 40.0;
            draw_weather_icon(&cr, 10.0, y, icon_size, weather_icon);
            
            // Weather info to the right of icon
            let info_x = 60.0;
            let font_desc = pango::FontDescription::from_string("Ubuntu 14");
            layout.set_font_description(Some(&font_desc));
            
            // Temperature
            if weather_temp > 0.0 {
                layout.set_text(&format!("{:.1}°C", weather_temp));
            } else {
                layout.set_text("N/A");
            }
            cr.move_to(info_x, y);
            pangocairo::functions::layout_path(&cr, &layout);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            cr.stroke_preserve().expect("Failed to stroke");
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.fill().expect("Failed to fill");
            
            // Description
            layout.set_text(weather_desc);
            cr.move_to(info_x, y + 20.0);
            pangocairo::functions::layout_path(&cr, &layout);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            cr.stroke_preserve().expect("Failed to stroke");
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.fill().expect("Failed to fill");
            
            // Location
            let location_font = pango::FontDescription::from_string("Ubuntu 12");
            layout.set_font_description(Some(&location_font));
            layout.set_text(weather_location);
            cr.move_to(info_x, y + 38.0);
            pangocairo::functions::layout_path(&cr, &layout);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            cr.stroke_preserve().expect("Failed to stroke");
            cr.set_source_rgb(0.7, 0.7, 0.7);
            cr.fill().expect("Failed to fill");
        }
    }
    
    // Ensure Cairo surface is flushed
    surface.flush();
}

impl MonitorWidget {
}

delegate_compositor!(MonitorWidget);
delegate_output!(MonitorWidget);
delegate_shm!(MonitorWidget);
delegate_seat!(MonitorWidget);
delegate_pointer!(MonitorWidget);
delegate_layer!(MonitorWidget);

delegate_registry!(MonitorWidget);

impl ProvidesRegistryState for MonitorWidget {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config_handler = cosmic_config::Config::new(
        "com.github.zoliviragh.CosmicMonitor",
        Config::VERSION,
    )?;
    
    let config = Config::get_entry(&config_handler).unwrap_or_default();
    
    eprintln!("Widget starting with position: X={}, Y={}", config.widget_x, config.widget_y);

    // Connect to Wayland
    let conn = Connection::connect_to_env()?;
    let (globals, mut event_queue) = registry_queue_init(&conn)?;
    let qh = event_queue.handle();

    // Create widget
    let mut widget = MonitorWidget::new(&globals, &qh, config, config_handler);
    widget.create_layer_surface(&qh);

    let mut last_draw = Instant::now();

    // Main event loop
    loop {
        let now = Instant::now();
        
        // Redraw every second for clock updates
        if now.duration_since(last_draw).as_secs() >= 1 {
            widget.draw(&qh);
            last_draw = now;
        }
        
        // Check for config updates every 500ms
        if now.duration_since(widget.last_config_check).as_millis() > 500 {
            widget.last_config_check = now;
            if let Ok(new_config) = Config::get_entry(&widget.config_handler) {
                // Only update if config actually changed
                if *widget.config != new_config {
                    // Update weather monitor if API key or location changed
                    if widget.config.weather_api_key != new_config.weather_api_key {
                        widget.weather.set_api_key(new_config.weather_api_key.clone());
                    }
                    if widget.config.weather_location != new_config.weather_location {
                        widget.weather.set_location(new_config.weather_location.clone());
                    }
                    
                    widget.config = Arc::new(new_config);
                    // Force a redraw
                    widget.draw(&qh);
                    last_draw = now; // Reset draw timer since we just drew
                }
            }
        }

        // Dispatch pending events without blocking
        event_queue.dispatch_pending(&mut widget)?;
        
        // Flush the connection
        event_queue.flush()?;
        
        // Sleep briefly to avoid busy-waiting
        std::thread::sleep(std::time::Duration::from_millis(100));

        if widget.exit {
            break;
        }
    }

    Ok(())
}
