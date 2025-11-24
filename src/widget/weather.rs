// SPDX-License-Identifier: MPL-2.0

//! Weather monitoring using OpenWeatherMap API

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Instant;

// Weather Icons font embedded in binary
const WEATHER_ICONS_FONT: &[u8] = include_bytes!("../../resources/weathericons.ttf");

// Load the Weather Icons font into Cairo/Pango
pub fn load_weather_font() {
    use std::io::Write;
    use std::fs;
    
    // Create a temporary file for the font (Pango needs a file path)
    let cache_dir = dirs::cache_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    let font_path = cache_dir.join("cosmic-monitor-weathericons.ttf");
    
    // Write font to cache if it doesn't exist or is outdated
    if !font_path.exists() || fs::metadata(&font_path).map(|m| m.len()).unwrap_or(0) != WEATHER_ICONS_FONT.len() as u64 {
        if let Ok(mut file) = fs::File::create(&font_path) {
            let _ = file.write_all(WEATHER_ICONS_FONT);
            log::info!("Weather Icons font loaded from embedded data to {:?}", font_path);
        }
    }
}

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
    pub weather_data: Arc<Mutex<Option<WeatherData>>>,
    pub last_update: Instant,
    api_key: Arc<Mutex<String>>,
    location: Arc<Mutex<String>>,
    update_requested: Arc<Mutex<bool>>,
}

impl WeatherMonitor {
    pub fn new(api_key: String, location: String) -> Self {
        // Initialize last_update to 11 minutes ago to force immediate first update
        let last_update = Instant::now() - std::time::Duration::from_secs(660);
        
        let api_key = Arc::new(Mutex::new(api_key));
        let location = Arc::new(Mutex::new(location));
        let update_requested = Arc::new(Mutex::new(false));
        let weather_data = Arc::new(Mutex::new(None));
        
        // Spawn background thread for weather updates
        let api_key_clone = Arc::clone(&api_key);
        let location_clone = Arc::clone(&location);
        let update_requested_clone = Arc::clone(&update_requested);
        let weather_data_clone = Arc::clone(&weather_data);
        
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(10));
                
                // Check if update is needed
                let requested = {
                    let mut req = update_requested_clone.lock().unwrap();
                    if *req {
                        *req = false;
                        true
                    } else {
                        false
                    }
                };
                
                if requested {
                    let api_key = api_key_clone.lock().unwrap().clone();
                    let location = location_clone.lock().unwrap().clone();
                    
                    if !api_key.is_empty() && !location.is_empty() {
                        log::info!("Background: Fetching weather data for location: {}", location);
                        match Self::fetch_weather_static(&api_key, &location) {
                            Ok(data) => {
                                log::info!("Background: Weather data fetched: {}°C, {} (icon: {})", data.temperature, data.description, data.icon);
                                *weather_data_clone.lock().unwrap() = Some(data);
                            }
                            Err(e) => {
                                log::error!("Background: Failed to fetch weather: {}", e);
                            }
                        }
                    }
                }
            }
        });
        
        Self {
            weather_data,
            last_update,
            api_key,
            location,
            update_requested,
        }
    }

    pub fn update(&mut self) {
        // Only update if we have an API key and location
        {
            let api_key = self.api_key.lock().unwrap();
            let location = self.location.lock().unwrap();
            
            if api_key.is_empty() || location.is_empty() {
                log::trace!("Weather update skipped: API key or location not configured");
                return;
            }
        }
        
        // Don't update more than once every 10 minutes
        let elapsed = self.last_update.elapsed().as_secs();
        if elapsed < 600 {
            log::trace!("Weather update skipped: too soon ({}s since last update, need 600s)", elapsed);
            return;
        }
        
        log::info!("Requesting weather update from background thread");
        *self.update_requested.lock().unwrap() = true;
        self.last_update = Instant::now();
    }
    
    fn fetch_weather_static(api_key: &str, location: &str) -> Result<WeatherData, Box<dyn std::error::Error>> {
        // Strip quotes from location and API key (cosmic_config may store them with quotes)
        let location = location.trim_matches('"');
        let api_key = api_key.trim_matches('"');
        
        log::debug!("Making API request for location: {}", location);
        
        let url = format!(
            "https://api.openweathermap.org/data/2.5/weather?q={}&appid={}&units=metric",
            location, api_key
        );

        // Use a client with timeout to prevent blocking indefinitely
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()?;
            
        let response: OpenWeatherResponse = client.get(&url).send()?.json()?;
        
        log::debug!("Weather API response received for: {}", response.name);

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
        *self.api_key.lock().unwrap() = api_key;
    }
    
    pub fn set_location(&mut self, location: String) {
        *self.location.lock().unwrap() = location;
    }
}

/// Draw a weather icon based on the OpenWeatherMap icon code
pub fn draw_weather_icon(cr: &cairo::Context, x: f64, y: f64, size: f64, icon_code: &str) {
    // Parse icon code: first 2 chars are condition, last char is day(d) or night(n)
    let condition = if icon_code.len() >= 2 { &icon_code[0..2] } else { "01" };
    let is_day = icon_code.ends_with('d');
    
    // Map OpenWeatherMap icon codes to Weather Icons font Unicode characters
    // Reference: https://erikflowers.github.io/weather-icons/
    let icon_char = match condition {
        "01" => if is_day { "\u{f00d}" } else { "\u{f02e}" },  // wi-day-sunny / wi-night-clear
        "02" => if is_day { "\u{f002}" } else { "\u{f086}" },  // wi-day-cloudy / wi-night-alt-cloudy
        "03" => if is_day { "\u{f013}" } else { "\u{f031}" },  // wi-day-sunny-overcast / wi-night-partly-cloudy
        "04" => "\u{f041}",                                     // wi-cloudy (same day/night)
        "09" => if is_day { "\u{f009}" } else { "\u{f029}" },  // wi-day-showers / wi-night-alt-showers
        "10" => if is_day { "\u{f008}" } else { "\u{f028}" },  // wi-day-rain / wi-night-alt-rain
        "11" => if is_day { "\u{f010}" } else { "\u{f02d}" },  // wi-day-thunderstorm / wi-night-alt-thunderstorm
        "13" => if is_day { "\u{f00a}" } else { "\u{f02a}" },  // wi-day-snow / wi-night-alt-snow
        "50" => if is_day { "\u{f003}" } else { "\u{f04a}" },  // wi-day-fog / wi-night-fog
        _ => "\u{f041}",                                        // Default to wi-cloudy
    };
    
    // Create pango layout for text rendering
    let layout = pangocairo::functions::create_layout(cr);
    
    // Use the Weather Icons font
    let font_desc = pango::FontDescription::from_string(&format!("Weather Icons {}", (size * 0.9) as i32));
    layout.set_font_description(Some(&font_desc));
    layout.set_text(icon_char);
    
    // Get text dimensions for centering
    let (text_width, text_height) = layout.pixel_size();
    
    // Center the icon
    let text_x = x + (size - text_width as f64) / 2.0;
    let text_y = y + (size - text_height as f64) / 2.0;
    
    cr.move_to(text_x, text_y);
    
    // Draw with white fill and black outline for visibility
    pangocairo::functions::layout_path(cr, &layout);
    cr.set_source_rgb(0.0, 0.0, 0.0);
    cr.set_line_width(3.0);
    cr.stroke_preserve().expect("Failed to stroke");
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.fill().expect("Failed to fill");
}
