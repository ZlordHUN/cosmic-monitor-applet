// SPDX-License-Identifier: MPL-2.0

//! Temperature monitoring (CPU, GPU)

use sysinfo::Components;

pub struct TemperatureMonitor {
    components: Components,
    pub cpu_temp: f32,
    pub gpu_temp: f32,
}

impl TemperatureMonitor {
    pub fn new() -> Self {
        Self {
            components: Components::new_with_refreshed_list(),
            cpu_temp: 0.0,
            gpu_temp: 0.0,
        }
    }

    pub fn update(&mut self) {
        self.components.refresh();
        
        // Try to find CPU temperature
        self.cpu_temp = 0.0;
        for component in &self.components {
            let label = component.label().to_lowercase();
            if label.contains("cpu") || label.contains("package") || label.contains("core") 
                || label.contains("tctl") || label.contains("tdie") {
                self.cpu_temp = component.temperature();
                break;
            }
        }
        
        // Try to find GPU temperature
        self.gpu_temp = 0.0;
        for component in &self.components {
            let label = component.label().to_lowercase();
            if label.contains("gpu") || label.contains("nvidia") || label.contains("amd") 
                || label.contains("radeon") || label.contains("edge") {
                self.gpu_temp = component.temperature();
                break;
            }
        }
    }
}

/// Draw a circular temperature gauge with color-changing hollow ring
pub fn draw_temp_circle(cr: &cairo::Context, x: f64, y: f64, radius: f64, temp: f32, max_temp: f32) {
    let center_x = x + radius;
    let center_y = y + radius;
    
    // Determine color based on temperature (similar to progress bar logic)
    let percentage = (temp / max_temp * 100.0).min(100.0);
    let (r, g, b) = if percentage < 50.0 {
        (0.4, 0.9, 0.4) // Green
    } else if percentage < 80.0 {
        (0.9, 0.9, 0.4) // Yellow
    } else {
        (0.9, 0.4, 0.4) // Red
    };
    
    // Draw outer ring (background)
    cr.arc(center_x, center_y, radius, 0.0, 2.0 * std::f64::consts::PI);
    cr.set_source_rgba(0.2, 0.2, 0.2, 0.7);
    cr.set_line_width(8.0);
    cr.stroke().expect("Failed to stroke");
    
    // Draw inner colored ring based on temperature
    let angle = (temp / max_temp).min(1.0) as f64 * 2.0 * std::f64::consts::PI;
    cr.arc(center_x, center_y, radius, -std::f64::consts::PI / 2.0, -std::f64::consts::PI / 2.0 + angle);
    cr.set_source_rgb(r, g, b);
    cr.set_line_width(8.0);
    cr.stroke().expect("Failed to stroke");
    
    // Draw border around the ring
    cr.arc(center_x, center_y, radius + 4.0, 0.0, 2.0 * std::f64::consts::PI);
    cr.set_source_rgb(0.0, 0.0, 0.0);
    cr.set_line_width(2.0);
    cr.stroke().expect("Failed to stroke");
    
    cr.arc(center_x, center_y, radius - 4.0, 0.0, 2.0 * std::f64::consts::PI);
    cr.set_source_rgb(0.0, 0.0, 0.0);
    cr.set_line_width(2.0);
    cr.stroke().expect("Failed to stroke");
}
