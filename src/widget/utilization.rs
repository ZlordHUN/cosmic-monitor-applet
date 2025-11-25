// SPDX-License-Identifier: MPL-2.0

//! CPU, Memory, and GPU Utilization Monitoring
//!
//! This module provides real-time system resource utilization monitoring for:
//! - **CPU**: Overall CPU usage percentage via sysinfo
//! - **Memory**: Used/total RAM with percentage
//! - **GPU**: Utilization for NVIDIA, AMD, and Intel GPUs
//!
//! # GPU Monitoring
//!
//! GPU utilization is monitored in a background thread to avoid blocking the UI.
//! The detection order is:
//!
//! 1. **NVIDIA**: Uses `nvidia-smi` command if available
//! 2. **AMD**: Reads from `/sys/class/drm/card*/device/gpu_busy_percent` (preferred)
//!    or falls back to `radeontop`
//! 3. **Intel**: Calculates from current/max frequency ratio in sysfs,
//!    or falls back to `intel_gpu_top`
//!
//! # Usage
//!
//! ```rust
//! let mut monitor = UtilizationMonitor::new();
//! 
//! // Call periodically (e.g., every second)
//! monitor.update();
//! 
//! println!("CPU: {:.1}%", monitor.cpu_usage);
//! println!("RAM: {:.1}%", monitor.memory_usage);
//! println!("GPU: {:.1}%", monitor.get_gpu_usage());
//! ```
//!
//! # Thread Safety
//!
//! GPU usage is stored in an `Arc<Mutex<f32>>` and updated by a background thread.
//! The `get_gpu_usage()` method safely reads the current value.

use sysinfo::System;
use std::process::Command;
use std::sync::{Arc, Mutex};

// ============================================================================
// GPU Vendor Detection
// ============================================================================

/// Supported GPU vendors for utilization monitoring.
#[derive(Debug, Clone, Copy, PartialEq)]
enum GpuVendor {
    /// NVIDIA GPU (uses nvidia-smi)
    Nvidia,
    /// AMD GPU (uses sysfs or radeontop)
    Amd,
    /// Intel integrated/discrete GPU (uses sysfs or intel_gpu_top)
    Intel,
    /// No supported GPU detected
    None,
}

// ============================================================================
// Main Monitor Structure
// ============================================================================

/// Monitors CPU, Memory, and GPU utilization.
///
/// CPU and Memory are updated synchronously via `update()`.
/// GPU utilization is monitored by a background thread for better accuracy.
pub struct UtilizationMonitor {
    /// sysinfo system instance for CPU/Memory data
    sys: System,
    
    /// Current CPU usage percentage (0-100)
    pub cpu_usage: f32,
    
    /// Current memory usage percentage (0-100)
    pub memory_usage: f32,
    
    /// Total system memory in bytes
    pub memory_total: u64,
    
    /// Used system memory in bytes
    pub memory_used: u64,
    
    /// GPU usage percentage, updated by background thread
    pub gpu_usage: Arc<Mutex<f32>>,
    
    /// Detected GPU vendor (determines monitoring method)
    gpu_vendor: GpuVendor,
}

// ============================================================================
// Implementation
// ============================================================================

impl UtilizationMonitor {
    /// Create a new utilization monitor.
    ///
    /// Automatically detects GPU vendor and spawns a background thread
    /// for GPU monitoring if a supported GPU is found.
    pub fn new() -> Self {
        // Shared GPU usage value for thread-safe access
        let gpu_usage = Arc::new(Mutex::new(0.0f32));
        
        // Detect which GPU monitoring method to use
        let gpu_vendor = Self::detect_gpu_vendor();
        
        // Spawn background thread for GPU monitoring (if GPU detected)
        if gpu_vendor != GpuVendor::None {
            let gpu_usage_clone = Arc::clone(&gpu_usage);
            std::thread::spawn(move || {
                loop {
                    // Poll every second for smooth updates
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    
                    let usage = match gpu_vendor {
                        GpuVendor::Nvidia => Self::fetch_nvidia_gpu_usage(),
                        GpuVendor::Amd => Self::fetch_amd_gpu_usage(),
                        GpuVendor::Intel => Self::fetch_intel_gpu_usage(),
                        GpuVendor::None => None,
                    };
                    
                    if let Some(usage) = usage {
                        *gpu_usage_clone.lock().unwrap() = usage;
                    }
                }
            });
        }
        
        Self {
            sys: System::new_all(),
            cpu_usage: 0.0,
            memory_usage: 0.0,
            memory_total: 0,
            memory_used: 0,
            gpu_usage,
            gpu_vendor,
        }
    }

    /// Update CPU and memory statistics.
    ///
    /// Should be called at the configured update interval (default: 1 second).
    /// GPU usage is updated by the background thread, not here.
    pub fn update(&mut self) {
        // Refresh CPU usage (requires multiple calls for accurate averaging)
        self.sys.refresh_cpu_all();
        self.cpu_usage = self.sys.global_cpu_usage();

        // Refresh memory statistics
        self.sys.refresh_memory();
        self.memory_used = self.sys.used_memory();
        self.memory_total = self.sys.total_memory();
        self.memory_usage = if self.memory_total > 0 {
            (self.memory_used as f32 / self.memory_total as f32) * 100.0
        } else {
            0.0
        };
        
        // Note: GPU usage is updated in background thread
    }
    
    /// Get current GPU usage percentage.
    ///
    /// Thread-safe read from the background-updated value.
    /// Returns 0.0 if no GPU is detected or monitoring failed.
    pub fn get_gpu_usage(&self) -> f32 {
        *self.gpu_usage.lock().unwrap()
    }
    
    // ========================================================================
    // GPU Vendor Detection
    // ========================================================================
    
    /// Detect which GPU vendor is present on the system.
    ///
    /// Checks for:
    /// 1. nvidia-smi binary (NVIDIA)
    /// 2. radeontop or rocm-smi (AMD)
    /// 3. intel_gpu_top (Intel)
    /// 4. sysfs driver detection (fallback)
    fn detect_gpu_vendor() -> GpuVendor {
        // Check for NVIDIA first (most common discrete GPU)
        if std::path::Path::new("/usr/bin/nvidia-smi").exists() {
            return GpuVendor::Nvidia;
        }
        
        // Check for AMD tools
        if std::path::Path::new("/usr/bin/radeontop").exists() 
            || std::path::Path::new("/opt/rocm/bin/rocm-smi").exists() {
            return GpuVendor::Amd;
        }
        
        // Check for Intel tools
        if std::path::Path::new("/usr/bin/intel_gpu_top").exists() {
            return GpuVendor::Intel;
        }
        
        // Fallback: Check sysfs for GPU driver information
        if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                
                // Look for card devices (card0, card1, etc.), not render nodes
                if name_str.starts_with("card") && !name_str.contains("-") {
                    if let Ok(device_path) = std::fs::read_link(entry.path()) {
                        let device_str = device_path.to_string_lossy();
                        if device_str.contains("amdgpu") {
                            return GpuVendor::Amd;
                        }
                        if device_str.contains("i915") {
                            return GpuVendor::Intel;
                        }
                    }
                }
            }
        }
        
        GpuVendor::None
    }
    
    // ========================================================================
    // GPU Usage Fetching (called from background thread)
    // ========================================================================
    
    /// Fetch NVIDIA GPU utilization via nvidia-smi.
    ///
    /// Parses the CSV output for GPU utilization percentage.
    fn fetch_nvidia_gpu_usage() -> Option<f32> {
        let output = Command::new("nvidia-smi")
            .arg("--query-gpu=utilization.gpu")
            .arg("--format=csv,noheader,nounits")
            .output();
        
        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                stdout.trim().parse::<f32>().ok()
            }
            _ => None,
        }
    }
    
    /// Fetch AMD GPU utilization.
    ///
    /// Prefers sysfs (no external tools needed), falls back to radeontop.
    fn fetch_amd_gpu_usage() -> Option<f32> {
        // Primary method: Read from sysfs (most reliable, no permissions needed)
        // AMD GPUs expose utilization in /sys/class/drm/card*/device/gpu_busy_percent
        if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                
                if name_str.starts_with("card") && !name_str.contains("-") {
                    let busy_path = entry.path().join("device/gpu_busy_percent");
                    if let Ok(content) = std::fs::read_to_string(&busy_path) {
                        if let Ok(usage) = content.trim().parse::<f32>() {
                            return Some(usage);
                        }
                    }
                }
            }
        }
        
        // Fallback: radeontop (requires permissions)
        if std::path::Path::new("/usr/bin/radeontop").exists() {
            let output = Command::new("radeontop")
                .arg("-d")
                .arg("-")
                .arg("-l")
                .arg("1")
                .output();
            
            if let Ok(output) = output {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    // Parse "gpu 45.67%" format
                    for line in stdout.lines() {
                        if line.contains("gpu") {
                            if let Some(percent_str) = line.split_whitespace().nth(1) {
                                if let Some(num_str) = percent_str.strip_suffix('%') {
                                    if let Ok(usage) = num_str.parse::<f32>() {
                                        return Some(usage);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        None
    }
    
    /// Fetch Intel GPU utilization.
    ///
    /// Calculates from frequency ratio (current/max), falls back to intel_gpu_top.
    fn fetch_intel_gpu_usage() -> Option<f32> {
        // Primary method: Calculate usage from frequency ratio
        // Intel GPUs expose frequency in sysfs
        if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                
                if name_str.starts_with("card") && !name_str.contains("-") {
                    // Try gt0 (most common)
                    let cur_freq_path = entry.path().join("gt/gt0/rps_cur_freq_mhz");
                    let max_freq_path = entry.path().join("gt/gt0/rps_max_freq_mhz");
                    
                    if let (Ok(cur_str), Ok(max_str)) = (
                        std::fs::read_to_string(&cur_freq_path),
                        std::fs::read_to_string(&max_freq_path)
                    ) {
                        if let (Ok(cur_freq), Ok(max_freq)) = (
                            cur_str.trim().parse::<f32>(),
                            max_str.trim().parse::<f32>()
                        ) {
                            if max_freq > 0.0 {
                                return Some((cur_freq / max_freq) * 100.0);
                            }
                        }
                    }
                }
            }
        }
        
        // Fallback: intel_gpu_top (requires CAP_PERFMON or root)
        if std::path::Path::new("/usr/bin/intel_gpu_top").exists() {
            let output = Command::new("intel_gpu_top")
                .arg("-J")
                .arg("-s")
                .arg("100")
                .output();
            
            if let Ok(output) = output {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    // Simple JSON parsing for "busy" field
                    if let Some(busy_idx) = stdout.find("\"busy\":") {
                        let after_busy = &stdout[busy_idx + 8..];
                        if let Some(end_idx) = after_busy.find(|c: char| !c.is_numeric() && c != '.') {
                            if let Ok(usage) = after_busy[..end_idx].parse::<f32>() {
                                return Some(usage);
                            }
                        }
                    }
                }
            }
        }
        
        None
    }
}

// ============================================================================
// Drawing Helper Functions
// ============================================================================
// These functions draw icons using Cairo for the utilization section.

/// Draw a CPU icon (chip with pins).
///
/// Used in the utilization section header.
pub fn draw_cpu_icon(cr: &cairo::Context, x: f64, y: f64, size: f64) {
    // Draw chip body
    cr.rectangle(x, y, size, size);
    cr.set_source_rgb(0.0, 0.0, 0.0);
    cr.set_line_width(2.0);
    cr.stroke_preserve().expect("Failed to stroke");
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.fill().expect("Failed to fill");
    
    // Draw pins on sides
    let pin_length = size * 0.2;
    let pin_spacing = size / 3.0;
    
    // Left pins
    for i in 0..3 {
        let py = y + pin_spacing * (i as f64 + 0.5);
        cr.move_to(x, py);
        cr.line_to(x - pin_length, py);
    }
    
    // Right pins
    for i in 0..3 {
        let py = y + pin_spacing * (i as f64 + 0.5);
        cr.move_to(x + size, py);
        cr.line_to(x + size + pin_length, py);
    }
    
    cr.set_source_rgb(0.0, 0.0, 0.0);
    cr.set_line_width(2.0);
    cr.stroke_preserve().expect("Failed to stroke");
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.stroke().expect("Failed to stroke");
}

/// Draw a RAM icon (simple memory chip representation)
pub fn draw_ram_icon(cr: &cairo::Context, x: f64, y: f64, size: f64) {
    // Draw memory stick body
    cr.rectangle(x, y + size * 0.2, size, size * 0.8);
    cr.set_source_rgb(0.0, 0.0, 0.0);
    cr.set_line_width(2.0);
    cr.stroke_preserve().expect("Failed to stroke");
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.fill().expect("Failed to fill");
    
    // Draw notch at top
    let notch_width = size * 0.3;
    let notch_x = x + (size - notch_width) / 2.0;
    cr.rectangle(notch_x, y, notch_width, size * 0.2);
    cr.set_source_rgb(0.0, 0.0, 0.0);
    cr.set_line_width(2.0);
    cr.stroke_preserve().expect("Failed to stroke");
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.fill().expect("Failed to fill");
    
    // Draw chips on the body
    let chip_size = size * 0.15;
    for i in 0..3 {
        let chip_y = y + size * 0.3 + i as f64 * size * 0.22;
        cr.rectangle(x + size * 0.15, chip_y, chip_size, chip_size);
        cr.rectangle(x + size * 0.55, chip_y, chip_size, chip_size);
    }
    cr.set_source_rgb(0.0, 0.0, 0.0);
    cr.set_line_width(1.5);
    cr.stroke().expect("Failed to stroke");
}

/// Draw a GPU icon (graphics card representation)
pub fn draw_gpu_icon(cr: &cairo::Context, x: f64, y: f64, size: f64) {
    // Draw GPU card body
    cr.rectangle(x, y + size * 0.3, size * 1.3, size * 0.7);
    cr.set_source_rgb(0.0, 0.0, 0.0);
    cr.set_line_width(2.0);
    cr.stroke_preserve().expect("Failed to stroke");
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.fill().expect("Failed to fill");
    
    // Draw fan (circle)
    cr.arc(x + size * 0.65, y + size * 0.65, size * 0.25, 0.0, 2.0 * std::f64::consts::PI);
    cr.set_source_rgb(0.0, 0.0, 0.0);
    cr.set_line_width(2.0);
    cr.stroke().expect("Failed to stroke");
    
    // Draw PCIe connector
    for i in 0..3 {
        let connector_x = x + i as f64 * size * 0.15;
        cr.rectangle(connector_x, y, size * 0.1, size * 0.25);
    }
    cr.set_source_rgb(0.0, 0.0, 0.0);
    cr.set_line_width(1.5);
    cr.stroke().expect("Failed to stroke");
}

/// Draw a horizontal progress bar
pub fn draw_progress_bar(cr: &cairo::Context, x: f64, y: f64, width: f64, height: f64, percentage: f32) {
    // Draw background
    cr.rectangle(x, y, width, height);
    cr.set_source_rgba(0.2, 0.2, 0.2, 0.7);
    cr.fill().expect("Failed to fill");
    
    // Draw border
    cr.rectangle(x, y, width, height);
    cr.set_source_rgb(0.0, 0.0, 0.0);
    cr.set_line_width(2.0);
    cr.stroke_preserve().expect("Failed to stroke");
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.set_line_width(1.0);
    cr.stroke().expect("Failed to stroke");
    
    // Draw filled portion
    let fill_width = width * (percentage / 100.0).min(1.0) as f64;
    if fill_width > 0.0 {
        cr.rectangle(x + 1.0, y + 1.0, fill_width - 2.0, height - 2.0);
        
        // Gradient fill based on percentage
        let pattern = cairo::LinearGradient::new(x, y, x + width, y);
        if percentage < 50.0 {
            pattern.add_color_stop_rgb(0.0, 0.4, 0.9, 0.4);
            pattern.add_color_stop_rgb(1.0, 0.4, 0.9, 0.4);
        } else if percentage < 80.0 {
            pattern.add_color_stop_rgb(0.0, 0.9, 0.9, 0.4);
            pattern.add_color_stop_rgb(1.0, 0.9, 0.9, 0.4);
        } else {
            pattern.add_color_stop_rgb(0.0, 0.9, 0.4, 0.4);
            pattern.add_color_stop_rgb(1.0, 0.9, 0.4, 0.4);
        }
        
        cr.set_source(&pattern).expect("Failed to set source");
        cr.fill().expect("Failed to fill");
    }
}
