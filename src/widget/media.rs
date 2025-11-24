// SPDX-License-Identifier: MPL-2.0

//! Media player monitoring via Cider API

use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

impl Default for PlaybackStatus {
    fn default() -> Self {
        PlaybackStatus::Stopped
    }
}

#[derive(Debug, Clone, Default)]
pub struct MediaInfo {
    pub player_name: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: Option<String>,
    pub status: PlaybackStatus,
    pub position: u64,      // Current position in milliseconds
    pub duration: u64,      // Track duration in milliseconds
    pub can_play: bool,
    pub can_pause: bool,
    pub can_go_next: bool,
    pub can_go_previous: bool,
    pub can_seek: bool,
}

impl MediaInfo {
    pub fn is_active(&self) -> bool {
        !self.player_name.is_empty() && !self.title.is_empty()
    }
    
    /// Get position as formatted time string (mm:ss)
    pub fn position_str(&self) -> String {
        let secs = self.position / 1000;
        format!("{}:{:02}", secs / 60, secs % 60)
    }
    
    /// Get duration as formatted time string (mm:ss)
    pub fn duration_str(&self) -> String {
        let secs = self.duration / 1000;
        format!("{}:{:02}", secs / 60, secs % 60)
    }
    
    /// Get progress as a fraction (0.0 to 1.0)
    pub fn progress(&self) -> f64 {
        if self.duration > 0 {
            (self.position as f64) / (self.duration as f64)
        } else {
            0.0
        }
    }
}

pub struct MediaMonitor {
    media_info: Arc<Mutex<MediaInfo>>,
    cider_token: Arc<Mutex<Option<String>>>,
}

impl MediaMonitor {
    pub fn new(api_token: Option<String>) -> Self {
        let media_info = Arc::new(Mutex::new(MediaInfo::default()));
        // Use provided token or None if empty
        let token = api_token.filter(|t| !t.is_empty());
        let cider_token = Arc::new(Mutex::new(token));
        
        // Spawn background thread to monitor media
        let media_info_clone = Arc::clone(&media_info);
        let cider_token_clone = Arc::clone(&cider_token);
        
        std::thread::spawn(move || {
            Self::monitor_loop(media_info_clone, cider_token_clone);
        });
        
        Self {
            media_info,
            cider_token,
        }
    }
    
    fn monitor_loop(
        media_info: Arc<Mutex<MediaInfo>>,
        cider_token: Arc<Mutex<Option<String>>>,
    ) {
        log::info!("Starting Cider media monitor");
        
        loop {
            // Try Cider API
            let token = cider_token.lock().unwrap().clone();
            if let Some(info) = Self::try_cider_api(token.as_deref()) {
                let mut stored = media_info.lock().unwrap();
                *stored = info;
            } else {
                // No media playing or Cider not running
                let mut stored = media_info.lock().unwrap();
                *stored = MediaInfo::default();
            }
            
            // Poll every second
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    
    fn try_cider_api(token: Option<&str>) -> Option<MediaInfo> {
        use std::process::Command;
        
        // Build curl command
        let mut cmd = Command::new("curl");
        cmd.args(&["-s", "--max-time", "1"]);
        
        if let Some(t) = token {
            cmd.args(&["-H", &format!("apptoken: {}", t)]);
        }
        
        cmd.arg("http://localhost:10767/api/v1/playback/now-playing");
        
        let output = cmd.output().ok()?;
        
        if !output.status.success() {
            return None;
        }
        
        let json_str = String::from_utf8_lossy(&output.stdout);
        
        // Check for error response
        if json_str.contains("\"error\"") {
            return None;
        }
        
        // Parse JSON response
        Self::parse_cider_response(&json_str)
    }
    
    fn parse_cider_response(json: &str) -> Option<MediaInfo> {
        // Check if status is ok
        if !json.contains("\"status\":\"ok\"") {
            return None;
        }
        
        let mut info = MediaInfo {
            player_name: "Cider".to_string(),
            can_play: true,
            can_pause: true,
            can_go_next: true,
            can_go_previous: true,
            can_seek: true,
            status: PlaybackStatus::Playing,
            ..Default::default()
        };
        
        // Extract title (name field)
        if let Some(name) = Self::extract_json_string(json, "\"name\":\"") {
            info.title = name;
        }
        
        // Extract artist
        if let Some(artist) = Self::extract_json_string(json, "\"artistName\":\"") {
            info.artist = artist;
        }
        
        // Extract album
        if let Some(album) = Self::extract_json_string(json, "\"albumName\":\"") {
            info.album = album;
        }
        
        // Extract artwork URL
        if let Some(artwork) = Self::extract_json_string(json, "\"url\":\"") {
            info.art_url = Some(artwork);
        }
        
        // Extract duration in milliseconds
        if let Some(duration_str) = Self::extract_json_number(json, "\"durationInMillis\":") {
            if let Ok(duration) = duration_str.parse::<u64>() {
                info.duration = duration;
            }
        }
        
        // Extract current playback time (in seconds, convert to ms)
        if let Some(pos_str) = Self::extract_json_number(json, "\"currentPlaybackTime\":") {
            if let Ok(pos) = pos_str.parse::<f64>() {
                info.position = (pos * 1000.0) as u64;
            }
        }
        
        // Check if we got meaningful data
        if info.title.is_empty() {
            return None;
        }
        
        Some(info)
    }
    
    fn extract_json_string(json: &str, key: &str) -> Option<String> {
        let start = json.find(key)? + key.len();
        let rest = &json[start..];
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    }
    
    fn extract_json_number(json: &str, key: &str) -> Option<String> {
        let start = json.find(key)? + key.len();
        let rest = &json[start..];
        let end = rest.find(|c: char| c == ',' || c == '}' || c == ']')?;
        Some(rest[..end].trim().to_string())
    }
    
    pub fn get_media_info(&self) -> MediaInfo {
        self.media_info.lock().unwrap().clone()
    }
    
    pub fn set_cider_token(&self, token: Option<String>) {
        let mut stored = self.cider_token.lock().unwrap();
        *stored = token;
        log::info!("Cider API token updated");
    }
    
    /// Send playback control via Cider API
    fn send_cider_command(&self, endpoint: &str) -> bool {
        use std::process::Command;
        
        let token = self.cider_token.lock().unwrap().clone();
        
        let mut cmd = Command::new("curl");
        cmd.args(&["-s", "-X", "POST", "--max-time", "1"]);
        
        if let Some(t) = token {
            cmd.args(&["-H", &format!("apptoken: {}", t)]);
        }
        
        cmd.arg(&format!("http://localhost:10767/api/v1/playback/{}", endpoint));
        
        if let Ok(output) = cmd.output() {
            return output.status.success();
        }
        false
    }
    
    pub fn play_pause(&self) {
        let info = self.get_media_info();
        if info.status == PlaybackStatus::Playing {
            self.send_cider_command("pause");
        } else {
            self.send_cider_command("play");
        }
    }
    
    pub fn next(&self) {
        self.send_cider_command("next");
    }
    
    pub fn previous(&self) {
        self.send_cider_command("previous");
    }
}
