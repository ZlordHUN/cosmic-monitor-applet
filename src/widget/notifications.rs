// SPDX-License-Identifier: MPL-2.0

//! # Notification Monitoring Module
//!
//! This module captures desktop notifications via D-Bus and displays them
//! in the widget. Uses `busctl` to monitor the `org.freedesktop.Notifications`
//! interface for incoming notification calls.
//!
//! ## D-Bus Interface
//!
//! Monitors the standard FreeDesktop Notifications specification:
//! ```text
//! Interface: org.freedesktop.Notifications
//! Method: Notify(app_name, replaces_id, app_icon, summary, body, actions, hints, expire_timeout)
//! ```
//!
//! ## Data Flow
//!
//! ```text
//! ┌──────────────┐    ┌─────────────┐    ┌───────────────┐
//! │ Desktop App  │───►│ D-Bus       │───►│ busctl        │
//! │ (notify-send)│    │ Notify call │    │ monitor       │
//! └──────────────┘    └─────────────┘    └───────┬───────┘
//!                                                 │
//!                     ┌───────────────┐          │ stdout
//!                     │ Main Thread   │◄─────────┘
//!                     │ (reads list)  │    ┌───────────────┐
//!                     └───────────────┘    │ Background    │
//!                                          │ Thread        │
//!                                          │ (parses)      │
//!                                          └───────────────┘
//! ```
//!
//! ## busctl Output Parsing
//!
//! The `busctl monitor` command outputs D-Bus messages in a text format.
//! We parse STRING fields from Notify method calls:
//!
//! ```text
//! Type=method_call  Member=Notify
//!   STRING "app_name"      # Index 0: Application name
//!   STRING ""              # Index 1: App icon (usually empty)
//!   STRING "Summary text"  # Index 2: Notification title
//!   STRING "Body text"     # Index 3: Notification body
//! ```
//!
//! ## Notification Management
//!
//! - New notifications are inserted at the front (newest first)
//! - List is capped at `max_notifications` to prevent unbounded growth
//! - Provides methods to clear all, clear by app, or remove specific notifications

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

// ============================================================================
// Notification Struct
// ============================================================================

/// A captured desktop notification.
///
/// Contains the essential fields from a D-Bus Notify method call,
/// plus a timestamp for ordering and identification.
#[derive(Debug, Clone)]
pub struct Notification {
    /// Application that sent the notification (e.g., "Firefox", "System")
    pub app_name: String,
    /// Notification title/headline
    pub summary: String,
    /// Notification body text (may be empty)
    pub body: String,
    /// Unix timestamp when notification was captured (seconds since epoch)
    pub timestamp: u64,
}

// ============================================================================
// Notification Monitor Struct
// ============================================================================

/// Monitors D-Bus for desktop notifications.
///
/// Spawns a background thread running `busctl monitor` to capture incoming
/// notifications. The notification list is shared via Arc<Mutex> for
/// thread-safe access from the main render thread.
///
/// # Threading Model
///
/// - Background thread: Runs `busctl monitor`, parses output, updates list
/// - Main thread: Reads notification list for rendering
/// - Shared state: `notifications` Vec protected by Mutex
///
/// # Resource Usage
///
/// - Spawns one persistent background thread
/// - Spawns one `busctl` child process
/// - Both run for the lifetime of the application
pub struct NotificationMonitor {
    /// Shared notification list, newest first
    notifications: Arc<Mutex<Vec<Notification>>>,
    /// Maximum number of notifications to keep (prevents unbounded growth)
    max_notifications: usize,
}

impl NotificationMonitor {
    /// Create a new notification monitor with background D-Bus listener.
    ///
    /// # Arguments
    ///
    /// * `max_notifications` - Maximum notifications to keep (oldest are dropped)
    ///
    /// # Background Thread
    ///
    /// Immediately spawns a background thread that:
    /// 1. Starts `busctl monitor` to watch D-Bus
    /// 2. Parses Notify method calls from stdout
    /// 3. Extracts app_name, summary, and body
    /// 4. Updates the shared notification list
    pub fn new(max_notifications: usize) -> Self {
        let notifications = Arc::new(Mutex::new(Vec::new()));
        
        // Spawn background thread to monitor D-Bus
        // This runs for the lifetime of the application
        let notifications_clone = Arc::clone(&notifications);
        let max_count = max_notifications;
        
        std::thread::spawn(move || {
            if let Err(e) = Self::monitor_notifications(notifications_clone, max_count) {
                log::error!("Notification monitoring error: {}", e);
            }
        });
        
        Self {
            notifications,
            max_notifications,
        }
    }
    
    /// Main D-Bus monitoring loop (runs in background thread).
    ///
    /// Uses `busctl monitor` to watch for Notify method calls on the
    /// user session bus. Parses the text output to extract notification
    /// fields.
    ///
    /// # busctl Command
    ///
    /// ```bash
    /// busctl monitor --user \
    ///   --match "type=method_call,interface=org.freedesktop.Notifications,member=Notify"
    /// ```
    ///
    /// # Parsing Strategy
    ///
    /// 1. Watch for lines containing "Member=Notify" to start new notification
    /// 2. Count STRING fields in order (app_name=0, icon=1, summary=2, body=3)
    /// 3. Extract values between double quotes
    /// 4. After body (field 3), save the notification
    ///
    /// # Error Handling
    ///
    /// Returns error if busctl cannot be spawned. Parsing errors within
    /// the loop are logged but don't stop monitoring.
    fn monitor_notifications(
        notifications: Arc<Mutex<Vec<Notification>>>,
        max_count: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use std::process::{Command, Stdio};
        use std::io::{BufRead, BufReader};
        
        log::info!("Starting notification monitor via busctl");
        
        // Use busctl to monitor D-Bus for Notify calls
        // --user: Watch user session bus (not system bus)
        // --match: Filter for only Notify method calls
        let mut child = Command::new("busctl")
            .args(&[
                "monitor",
                "--user",
                "--match",
                "type=method_call,interface=org.freedesktop.Notifications,member=Notify",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())  // Suppress busctl stderr noise
            .spawn()?;
        
        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let reader = BufReader::new(stdout);
        
        // State machine for parsing busctl output
        let mut current_app_name = String::new();
        let mut current_summary = String::new();
        let mut current_body = String::new();
        let mut string_field_index = 0;  // Track which STRING field we're at
        let mut in_notify_call = false;  // Are we parsing a Notify call?
        
        // Process busctl output line by line
        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            
            // busctl output format: look for Notify method call header
            if trimmed.contains("Member=Notify") {
                // Reset state for new notification
                current_app_name.clear();
                current_summary.clear();
                current_body.clear();
                string_field_index = 0;
                in_notify_call = true;
            } else if in_notify_call && trimmed.starts_with("STRING \"") {
                // Extract string value between quotes
                // Format: STRING "value here"
                if let Some(start) = trimmed.find('"') {
                    if let Some(end) = trimmed.rfind('"') {
                        if start < end {
                            let value = &trimmed[start + 1..end];
                            
                            // Notify STRING parameters in order:
                            // 0: app_name - Application sending the notification
                            // 1: app_icon - Icon name or path (usually empty)
                            // 2: summary - Notification title
                            // 3: body - Notification body text
                            match string_field_index {
                                0 => current_app_name = value.to_string(),
                                2 => current_summary = value.to_string(),
                                3 => {
                                    current_body = value.to_string();
                                    in_notify_call = false;  // Done parsing this call
                                    
                                    // We have all the data, create notification
                                    if !current_summary.is_empty() {
                                        let timestamp = SystemTime::now()
                                            .duration_since(UNIX_EPOCH)
                                            .unwrap()
                                            .as_secs();
                                        
                                        let notification = Notification {
                                            app_name: if current_app_name.is_empty() { 
                                                "System".to_string()  // Fallback for empty app_name
                                            } else { 
                                                current_app_name.clone() 
                                            },
                                            summary: current_summary.clone(),
                                            body: current_body.clone(),
                                            timestamp,
                                        };
                                        
                                        log::info!("Captured notification: {} - {}", 
                                            notification.app_name, notification.summary);
                                        
                                        // Insert at front (newest first) and truncate if needed
                                        let mut notifs = notifications.lock().unwrap();
                                        notifs.insert(0, notification);
                                        
                                        if notifs.len() > max_count {
                                            notifs.truncate(max_count);
                                        }
                                    }
                                }
                                _ => {}  // Ignore other STRING fields (icon, etc.)
                            }
                            string_field_index += 1;
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Get a snapshot of current notifications (newest first).
    ///
    /// Returns a clone of the notification list for safe iteration
    /// without holding the lock.
    pub fn get_notifications(&self) -> Vec<Notification> {
        self.notifications.lock().unwrap().clone()
    }
    
    /// Clear all notifications.
    ///
    /// Removes all notifications from the list. Does not affect the
    /// underlying D-Bus monitoring (new notifications will still appear).
    pub fn clear(&self) {
        let mut notifs = self.notifications.lock().unwrap();
        notifs.clear();
        log::info!("Cleared all notifications");
    }
    
    /// Clear all notifications from a specific application.
    ///
    /// # Arguments
    ///
    /// * `app_name` - Application name to filter (exact match)
    pub fn clear_app(&self, app_name: &str) {
        let mut notifs = self.notifications.lock().unwrap();
        notifs.retain(|n| n.app_name != app_name);
        log::info!("Cleared notifications for app: {}", app_name);
    }
    
    /// Remove a specific notification by app name and timestamp.
    ///
    /// Used when the user clicks the X button on a specific notification.
    ///
    /// # Arguments
    ///
    /// * `app_name` - Application name of the notification
    /// * `timestamp` - Unix timestamp when notification was captured
    pub fn remove_notification(&self, app_name: &str, timestamp: u64) {
        let mut notifs = self.notifications.lock().unwrap();
        notifs.retain(|n| !(n.app_name == app_name && n.timestamp == timestamp));
        log::info!("Removed notification: {} at {}", app_name, timestamp);
    }
}

