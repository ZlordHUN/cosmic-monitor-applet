// SPDX-License-Identifier: MPL-2.0

//! Weather monitoring using OpenWeatherMap API

use serde::{Deserialize, Serialize};
use std::time::Instant;

// OpenWeatherMap API response structures
#[derive(Debug, Deserialize)]
struct OpenWeatherResponse {
    main: MainWeather,
    weather: Vec<WeatherCondition>,
    name: String,
}

#[derive(Debug, Deserialize)]
struct MainWeather {
    temp: f32,
    feels_like: f32,
    temp_min: f32,
    temp_max: f32,
    humidity: u8,
}

#[derive(Debug, Deserialize)]
struct WeatherCondition {
    description: String,
    icon: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherData {
    pub temperature: f32,
    pub feels_like: f32,
    pub temp_min: f32,
    pub temp_max: f32,
    pub humidity: u8,
    pub description: String,
    pub icon: String,
    pub location: String,
}

impl Default for WeatherData {
    fn default() -> Self {
        Self {
            temperature: 0.0,
            feels_like: 0.0,
            temp_min: 0.0,
            temp_max: 0.0,
            humidity: 0,
            description: String::from("N/A"),
            icon: String::from("01d"),
            location: String::from("Unknown"),
        }
    }
}

pub struct WeatherMonitor {
    pub weather_data: Option<WeatherData>,
    pub last_update: Instant,
    api_key: String,
    location: String,
}

impl WeatherMonitor {
    pub fn new(api_key: String, location: String) -> Self {
        // Initialize last_update to 11 minutes ago to force immediate first update
        let last_update = Instant::now() - std::time::Duration::from_secs(660);
        
        Self {
            weather_data: None,
            last_update,
            api_key,
            location,
        }
    }

    pub fn update(&mut self) {
        // Only update if we have an API key and location
        if self.api_key.is_empty() || self.location.is_empty() {
            return;
        }

        // Don't update more than once every 10 minutes
        if self.last_update.elapsed().as_secs() < 600 {
            return;
        }
        
        // Fetch weather data synchronously (blocking)
        // Note: This blocks the thread, but updates are infrequent (every 10 minutes)
        match self.fetch_weather() {
            Ok(data) => {
                self.weather_data = Some(data);
                self.last_update = Instant::now();
            }
            Err(_e) => {
                // Silently fail - weather data will remain stale or empty
            }
        }
    }

    fn fetch_weather(&self) -> Result<WeatherData, Box<dyn std::error::Error>> {
        // Strip quotes from location and API key (cosmic_config may store them with quotes)
        let location = self.location.trim_matches('"');
        let api_key = self.api_key.trim_matches('"');
        
        let url = format!(
            "https://api.openweathermap.org/data/2.5/weather?q={}&appid={}&units=metric",
            location, api_key
        );

        let response: OpenWeatherResponse = reqwest::blocking::get(&url)?.json()?;

        let description = response
            .weather
            .first()
            .map(|w| {
                let mut desc = w.description.clone();
                if let Some(first_char) = desc.chars().next() {
                    desc = first_char.to_uppercase().collect::<String>() + &desc[1..];
                }
                desc
            })
            .unwrap_or_else(|| String::from("Unknown"));

        let icon = response
            .weather
            .first()
            .map(|w| w.icon.clone())
            .unwrap_or_else(|| String::from("01d"));

        Ok(WeatherData {
            temperature: response.main.temp,
            feels_like: response.main.feels_like,
            temp_min: response.main.temp_min,
            temp_max: response.main.temp_max,
            humidity: response.main.humidity,
            description,
            icon,
            location: response.name,
        })
    }
    
    pub fn set_api_key(&mut self, api_key: String) {
        self.api_key = api_key;
    }
    
    pub fn set_location(&mut self, location: String) {
        self.location = location;
    }
}

/// Draw a weather icon based on the OpenWeatherMap icon code
pub fn draw_weather_icon(cr: &cairo::Context, x: f64, y: f64, size: f64, icon_code: &str) {
    let center_x = x + size / 2.0;
    let center_y = y + size / 2.0;
    
    // Parse icon code: first 2 chars are condition, last char is day(d) or night(n)
    let condition = if icon_code.len() >= 2 { &icon_code[0..2] } else { "01" };
    let is_day = icon_code.ends_with('d');
    
    match condition {
        // Clear sky
        "01" => {
            if is_day {
                // Sun
                let radius = size * 0.25;
                
                // Draw sun circle
                cr.arc(center_x, center_y, radius, 0.0, 2.0 * std::f64::consts::PI);
                cr.set_source_rgb(1.0, 0.9, 0.0);
                cr.fill().expect("Failed to fill");
                
                // Draw sun rays
                for i in 0..8 {
                    let angle = (i as f64) * std::f64::consts::PI / 4.0;
                    let inner_x = center_x + (radius * 1.3) * angle.cos();
                    let inner_y = center_y + (radius * 1.3) * angle.sin();
                    let outer_x = center_x + (radius * 1.7) * angle.cos();
                    let outer_y = center_y + (radius * 1.7) * angle.sin();
                    
                    cr.move_to(inner_x, inner_y);
                    cr.line_to(outer_x, outer_y);
                }
                cr.set_source_rgb(1.0, 0.9, 0.0);
                cr.set_line_width(2.0);
                cr.stroke().expect("Failed to stroke");
                
                // Draw border
                cr.arc(center_x, center_y, radius, 0.0, 2.0 * std::f64::consts::PI);
                cr.set_source_rgb(0.0, 0.0, 0.0);
                cr.set_line_width(1.5);
                cr.stroke().expect("Failed to stroke");
            } else {
                // Moon
                let radius = size * 0.25;
                
                // Draw full moon circle
                cr.arc(center_x, center_y, radius, 0.0, 2.0 * std::f64::consts::PI);
                cr.set_source_rgb(0.9, 0.9, 0.7);
                cr.fill().expect("Failed to fill");
                
                // Draw crescent shadow
                let shadow_offset = radius * 0.4;
                cr.arc(center_x + shadow_offset, center_y, radius * 0.9, 0.0, 2.0 * std::f64::consts::PI);
                cr.set_source_rgb(0.3, 0.3, 0.3);
                cr.fill().expect("Failed to fill");
                
                // Draw border
                cr.arc(center_x, center_y, radius, 0.0, 2.0 * std::f64::consts::PI);
                cr.set_source_rgb(0.0, 0.0, 0.0);
                cr.set_line_width(1.5);
                cr.stroke().expect("Failed to stroke");
            }
        },
        
        // Few clouds
        "02" => {
            draw_cloud_icon(cr, center_x, center_y + size * 0.1, size * 0.6);
            if is_day {
                draw_small_sun(cr, center_x - size * 0.15, center_y - size * 0.15, size * 0.35);
            } else {
                draw_small_moon(cr, center_x - size * 0.15, center_y - size * 0.15, size * 0.35);
            }
        },
        
        // Scattered clouds / broken clouds / overcast
        "03" | "04" => {
            draw_cloud_icon(cr, center_x, center_y, size * 0.7);
        },
        
        // Rain / shower rain
        "09" | "10" => {
            draw_cloud_icon(cr, center_x, center_y - size * 0.1, size * 0.6);
            draw_rain_drops(cr, center_x, center_y + size * 0.2, size * 0.6);
            if condition == "10" && is_day {
                draw_small_sun(cr, center_x - size * 0.2, center_y - size * 0.2, size * 0.3);
            }
        },
        
        // Thunderstorm
        "11" => {
            draw_cloud_icon(cr, center_x, center_y - size * 0.1, size * 0.6);
            draw_lightning(cr, center_x, center_y + size * 0.15, size * 0.4);
        },
        
        // Snow
        "13" => {
            draw_cloud_icon(cr, center_x, center_y - size * 0.1, size * 0.6);
            draw_snowflakes(cr, center_x, center_y + size * 0.2, size * 0.6);
        },
        
        // Mist / fog
        "50" => {
            draw_fog_lines(cr, center_x, center_y, size * 0.7);
        },
        
        _ => {
            // Default to cloud
            draw_cloud_icon(cr, center_x, center_y, size * 0.7);
        }
    }
}

fn draw_cloud_icon(cr: &cairo::Context, x: f64, y: f64, size: f64) {
    let radius1 = size * 0.2;
    let radius2 = size * 0.25;
    let radius3 = size * 0.2;
    
    // Draw three overlapping circles to form a cloud
    cr.arc(x - size * 0.15, y, radius1, 0.0, 2.0 * std::f64::consts::PI);
    cr.arc(x, y - size * 0.05, radius2, 0.0, 2.0 * std::f64::consts::PI);
    cr.arc(x + size * 0.15, y, radius3, 0.0, 2.0 * std::f64::consts::PI);
    
    cr.set_source_rgb(0.85, 0.85, 0.85);
    cr.fill_preserve().expect("Failed to fill");
    cr.set_source_rgb(0.0, 0.0, 0.0);
    cr.set_line_width(1.5);
    cr.stroke().expect("Failed to stroke");
}

fn draw_small_sun(cr: &cairo::Context, x: f64, y: f64, size: f64) {
    let radius = size * 0.4;
    
    cr.arc(x, y, radius, 0.0, 2.0 * std::f64::consts::PI);
    cr.set_source_rgb(1.0, 0.9, 0.0);
    cr.fill().expect("Failed to fill");
    
    // Simple rays
    for i in 0..4 {
        let angle = (i as f64) * std::f64::consts::PI / 2.0;
        let inner_x = x + (radius * 1.2) * angle.cos();
        let inner_y = y + (radius * 1.2) * angle.sin();
        let outer_x = x + (radius * 1.5) * angle.cos();
        let outer_y = y + (radius * 1.5) * angle.sin();
        
        cr.move_to(inner_x, inner_y);
        cr.line_to(outer_x, outer_y);
    }
    cr.set_line_width(1.5);
    cr.stroke().expect("Failed to stroke");
    
    cr.arc(x, y, radius, 0.0, 2.0 * std::f64::consts::PI);
    cr.set_source_rgb(0.0, 0.0, 0.0);
    cr.set_line_width(1.0);
    cr.stroke().expect("Failed to stroke");
}

fn draw_small_moon(cr: &cairo::Context, x: f64, y: f64, size: f64) {
    let radius = size * 0.4;
    
    cr.arc(x, y, radius, 0.0, 2.0 * std::f64::consts::PI);
    cr.set_source_rgb(0.9, 0.9, 0.7);
    cr.fill().expect("Failed to fill");
    
    cr.arc(x, y, radius, 0.0, 2.0 * std::f64::consts::PI);
    cr.set_source_rgb(0.0, 0.0, 0.0);
    cr.set_line_width(1.0);
    cr.stroke().expect("Failed to stroke");
}

fn draw_rain_drops(cr: &cairo::Context, x: f64, y: f64, size: f64) {
    // Draw 4 rain drops
    for i in 0..4 {
        let drop_x = x + (i as f64 - 1.5) * size * 0.2;
        let drop_y = y + if i % 2 == 0 { 0.0 } else { size * 0.15 };
        
        cr.move_to(drop_x, drop_y);
        cr.line_to(drop_x, drop_y + size * 0.2);
        cr.set_source_rgb(0.3, 0.5, 1.0);
        cr.set_line_width(2.0);
        cr.stroke().expect("Failed to stroke");
    }
}

fn draw_lightning(cr: &cairo::Context, x: f64, y: f64, size: f64) {
    cr.move_to(x, y);
    cr.line_to(x - size * 0.15, y + size * 0.25);
    cr.line_to(x + size * 0.05, y + size * 0.25);
    cr.line_to(x - size * 0.1, y + size * 0.5);
    cr.line_to(x + size * 0.15, y + size * 0.15);
    cr.line_to(x + size * 0.05, y + size * 0.15);
    cr.close_path();
    
    cr.set_source_rgb(1.0, 0.9, 0.0);
    cr.fill_preserve().expect("Failed to fill");
    cr.set_source_rgb(0.0, 0.0, 0.0);
    cr.set_line_width(1.5);
    cr.stroke().expect("Failed to stroke");
}

fn draw_snowflakes(cr: &cairo::Context, x: f64, y: f64, size: f64) {
    // Draw 3 simple snowflakes
    for i in 0..3 {
        let flake_x = x + (i as f64 - 1.0) * size * 0.25;
        let flake_y = y + if i % 2 == 0 { 0.0 } else { size * 0.15 };
        let flake_size = size * 0.1;
        
        // Draw a simple 6-pointed star
        for j in 0..6 {
            let angle = (j as f64) * std::f64::consts::PI / 3.0;
            cr.move_to(flake_x, flake_y);
            cr.line_to(
                flake_x + flake_size * angle.cos(),
                flake_y + flake_size * angle.sin()
            );
        }
        cr.set_source_rgb(0.7, 0.9, 1.0);
        cr.set_line_width(1.5);
        cr.stroke().expect("Failed to stroke");
    }
}

fn draw_fog_lines(cr: &cairo::Context, x: f64, y: f64, size: f64) {
    // Draw 3 horizontal wavy lines
    for i in 0..3 {
        let line_y = y + (i as f64 - 1.0) * size * 0.25;
        cr.move_to(x - size * 0.3, line_y);
        cr.line_to(x + size * 0.3, line_y);
        cr.set_source_rgb(0.7, 0.7, 0.7);
        cr.set_line_width(3.0);
        cr.stroke().expect("Failed to stroke");
    }
}
