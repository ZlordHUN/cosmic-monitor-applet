// SPDX-License-Identifier: MPL-2.0

//! # Network Monitoring Module
//!
//! This module tracks network throughput (upload/download speeds) across all
//! network interfaces using the `sysinfo` crate.
//!
//! ## Measurement Approach
//!
//! Network speed is calculated by measuring the change in total bytes
//! transferred over time:
//!
//! ```text
//! Rate (bytes/sec) = (current_bytes - previous_bytes) / elapsed_time
//! ```
//!
//! The module aggregates traffic from ALL network interfaces (eth0, wlan0,
//! docker0, lo, etc.) to give a system-wide throughput view.
//!
//! ## Data Sources
//!
//! - **sysinfo crate**: Reads from `/proc/net/dev` or equivalent
//! - **Byte counters**: Cumulative since boot (wraps at 2^64)
//!
//! ## Display Format
//!
//! Rates are converted to human-readable units in the renderer:
//! - KB/s for speeds < 1 MB/s
//! - MB/s for speeds ≥ 1 MB/s
//!
//! ## Edge Cases Handled
//!
//! - **Counter reset**: Kernel updates or interface restarts reset counters to 0
//! - **First update**: No previous data, so rate starts at 0
//! - **Interface changes**: New interfaces are automatically included on refresh

use sysinfo::Networks;
use std::time::Instant;

// ============================================================================
// Network Monitor Struct
// ============================================================================

/// Monitors network throughput across all interfaces.
///
/// Calculates download (RX) and upload (TX) speeds in bytes per second by
/// tracking the change in cumulative byte counters over time.
///
/// # Fields
///
/// - `networks`: sysinfo's network interface list
/// - `network_rx_bytes`: Previous total received bytes (for delta calculation)
/// - `network_tx_bytes`: Previous total transmitted bytes (for delta calculation)
/// - `network_rx_rate`: Current download speed in bytes/second
/// - `network_tx_rate`: Current upload speed in bytes/second
/// - `last_update`: Timestamp of last update (for elapsed time calculation)
///
/// # Rate Calculation
///
/// ```text
/// rx_rate = (current_rx - previous_rx) / seconds_elapsed
/// tx_rate = (current_tx - previous_tx) / seconds_elapsed
/// ```
pub struct NetworkMonitor {
    /// sysinfo's network interface list (refreshed on update)
    networks: Networks,
    /// Previous total received bytes across all interfaces
    network_rx_bytes: u64,
    /// Previous total transmitted bytes across all interfaces
    network_tx_bytes: u64,
    /// Current download rate in bytes per second
    pub network_rx_rate: f64,
    /// Current upload rate in bytes per second
    pub network_tx_rate: f64,
    /// Timestamp of last update for elapsed time calculation
    last_update: Instant,
}

impl NetworkMonitor {
    /// Create a new network monitor.
    ///
    /// Initializes sysinfo's network list with immediate discovery of all
    /// interfaces. Initial rates are 0.0 until the second update provides
    /// a delta for calculation.
    pub fn new() -> Self {
        Self {
            networks: Networks::new_with_refreshed_list(),
            network_rx_bytes: 0,
            network_tx_bytes: 0,
            network_rx_rate: 0.0,
            network_tx_rate: 0.0,
            last_update: Instant::now(),
        }
    }

    /// Update network throughput calculations.
    ///
    /// Refreshes sysinfo's network data, sums bytes across all interfaces,
    /// then calculates the rate based on time elapsed since last update.
    ///
    /// # Algorithm
    ///
    /// 1. Calculate elapsed time since last update
    /// 2. Refresh network interface data
    /// 3. Sum RX and TX bytes across ALL interfaces
    /// 4. Calculate rates: `(new_bytes - old_bytes) / elapsed_seconds`
    /// 5. Store new byte counts for next delta calculation
    ///
    /// # Counter Reset Handling
    ///
    /// If byte counters appear to have decreased (system reboot, interface
    /// restart, or first update), rates are reset to 0 to avoid showing
    /// incorrect negative or astronomical values.
    pub fn update(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        
        // Refresh network statistics from /proc/net/dev
        self.networks.refresh();
        
        // Sum bytes from ALL network interfaces (eth0, wlan0, docker0, lo, etc.)
        let mut total_rx = 0;
        let mut total_tx = 0;
        for (_interface_name, network) in &self.networks {
            total_rx += network.received();
            total_tx += network.transmitted();
        }
        
        // Handle counter resets (e.g., after kernel update or interface restart)
        // Only calculate rates if counters have increased since last update
        if self.network_rx_bytes > 0 && total_rx >= self.network_rx_bytes && total_tx >= self.network_tx_bytes {
            // Normal case: calculate bytes per second
            self.network_rx_rate = (total_rx - self.network_rx_bytes) as f64 / elapsed;
            self.network_tx_rate = (total_tx - self.network_tx_bytes) as f64 / elapsed;
        } else {
            // Counter was reset or this is the first update, reset rates to 0
            self.network_rx_rate = 0.0;
            self.network_tx_rate = 0.0;
        }
        
        // Store current values for next update's delta calculation
        self.network_rx_bytes = total_rx;
        self.network_tx_bytes = total_tx;
        self.last_update = now;
    }
}
