// SPDX-License-Identifier: MPL-2.0

//! Network monitoring

use sysinfo::Networks;
use std::time::Instant;

pub struct NetworkMonitor {
    networks: Networks,
    network_rx_bytes: u64,
    network_tx_bytes: u64,
    pub network_rx_rate: f64,
    pub network_tx_rate: f64,
    last_update: Instant,
}

impl NetworkMonitor {
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

    pub fn update(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        
        self.networks.refresh();
        
        let mut total_rx = 0;
        let mut total_tx = 0;
        for (_interface_name, network) in &self.networks {
            total_rx += network.received();
            total_tx += network.transmitted();
        }
        
        if self.network_rx_bytes > 0 {
            self.network_rx_rate = (total_rx - self.network_rx_bytes) as f64 / elapsed;
            self.network_tx_rate = (total_tx - self.network_tx_bytes) as f64 / elapsed;
        }
        self.network_rx_bytes = total_rx;
        self.network_tx_bytes = total_tx;
        self.last_update = now;
    }
}
