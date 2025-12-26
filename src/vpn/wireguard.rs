use std::path::Path;
use std::process::{Command, Stdio};
use tracing::{debug, error, info, warn};
use crate::errors::HvtError;
use crate::config::WireGuardConfig;

pub struct WireGuardManager {
    interface_name: String,
    config_path: String,
    connected: bool,
    is_windows: bool,
}

impl WireGuardManager {
    /// Create a new WireGuard manager from configuration
    pub fn new(config: &WireGuardConfig) -> Result<Self, HvtError> {
        let config_path = config.config_path.clone();

        // Determine interface name
        let interface_name = if let Some(name) = &config.interface_name {
            name.clone()
        } else {
            // Extract from config filename (e.g., "wg-japan.conf" -> "wg-japan")
            Path::new(&config_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| HvtError::Generic("Invalid WireGuard config path".to_string()))?
                .to_string()
        };

        // Validate config file exists
        if !Path::new(&config_path).exists() {
            return Err(HvtError::Generic(format!(
                "WireGuard config file not found: {}",
                config_path
            )));
        }

        Ok(Self {
            interface_name,
            config_path,
            connected: false,
            is_windows: cfg!(target_os = "windows"),
        })
    }

    /// Bring up the WireGuard interface
    pub fn connect(&mut self) -> Result<(), HvtError> {
        if self.connected {
            debug!("WireGuard already connected on interface {}", self.interface_name);
            return Ok(());
        }

        info!("Connecting WireGuard (interface: {})...", self.interface_name);

        // First, check if the interface already exists
        if self.interface_exists()? {
            info!("WireGuard interface {} already active, reusing it", self.interface_name);
            self.connected = true;
            return Ok(());
        }

        // Platform-specific connection
        if self.is_windows {
            self.connect_windows()?;
        } else {
            self.connect_unix()?;
        }

        self.connected = true;

        // Wait for the interface and routing to be ready
        // Network routing needs time to stabilize after interface creation
        debug!("Waiting for network routes to stabilize...");
        std::thread::sleep(std::time::Duration::from_secs(3));

        // Verify connection
        self.verify_connection()?;

        // Test network connectivity
        debug!("Testing network connectivity...");
        self.test_connectivity()?;

        info!("WireGuard connected successfully!");
        Ok(())
    }

    /// Connect WireGuard on Unix systems (Linux/macOS) using wg-quick
    fn connect_unix(&mut self) -> Result<(), HvtError> {
        // Check if wg-quick is available
        self.check_wg_quick_available()?;

        // Try to bring up the interface using wg-quick
        let output = Command::new("sudo")
            .args(&["wg-quick", "up", &self.config_path])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| HvtError::Generic(format!("Failed to execute wg-quick: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);

            // Check if interface is already up (race condition)
            if stderr.contains("already exists") {
                info!("WireGuard interface {} already active (race condition)", self.interface_name);
                return Ok(());
            }

            return Err(HvtError::Generic(format!(
                "Failed to bring up WireGuard interface: {}",
                stderr
            )));
        }

        Ok(())
    }

    /// Connect WireGuard on Windows using wireguard.exe
    fn connect_windows(&mut self) -> Result<(), HvtError> {
        // Check if wireguard.exe is available
        self.check_wireguard_windows_available()?;

        // On Windows, use: wireguard.exe /installtunnelservice <config-path>
        let output = Command::new("wireguard.exe")
            .args(&["/installtunnelservice", &self.config_path])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| HvtError::Generic(format!("Failed to execute wireguard.exe: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);

            // Check if tunnel is already installed
            if stderr.contains("already exists") || stderr.contains("already installed") {
                info!("WireGuard tunnel {} already active", self.interface_name);
                return Ok(());
            }

            return Err(HvtError::Generic(format!(
                "Failed to install WireGuard tunnel: {}",
                stderr
            )));
        }

        Ok(())
    }

    /// Check if the WireGuard interface already exists
    pub fn interface_exists(&self) -> Result<bool, HvtError> {
        if self.is_windows {
            // On Windows, check if tunnel service is running
            let output = Command::new("sc")
                .args(&["query", &format!("WireGuardTunnel${}", self.interface_name)])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .map_err(|e| HvtError::Generic(format!("Failed to check interface: {}", e)))?;

            Ok(output.status.success())
        } else {
            // On Unix, use wg show
            let output = Command::new("sudo")
                .args(&["wg", "show", &self.interface_name])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .map_err(|e| HvtError::Generic(format!("Failed to check interface: {}", e)))?;

            Ok(output.status.success())
        }
    }

    /// Bring down the WireGuard interface
    pub fn disconnect(&mut self) -> Result<(), HvtError> {
        if !self.connected {
            return Ok(());
        }

        info!("Disconnecting WireGuard (interface: {})...", self.interface_name);

        if self.is_windows {
            self.disconnect_windows()?;
        } else {
            self.disconnect_unix()?;
        }

        self.connected = false;
        info!("WireGuard disconnected");
        Ok(())
    }

    /// Disconnect WireGuard on Unix systems
    fn disconnect_unix(&mut self) -> Result<(), HvtError> {
        let output = Command::new("sudo")
            .args(&["wg-quick", "down", &self.config_path])
            //.stdout(Stdio::piped())
            //.stderr(Stdio::piped())
            .output()
            .map_err(|e| HvtError::Generic(format!("Failed to execute wg-quick: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);

            // If interface doesn't exist, consider it already down
            if stderr.contains("does not exist") || stderr.contains("Cannot find device") {
                return Ok(());
            }

            warn!("Failed to bring down WireGuard interface: {}", stderr);
        }

        Ok(())
    }

    /// Disconnect WireGuard on Windows
    fn disconnect_windows(&mut self) -> Result<(), HvtError> {
        let output = Command::new("wireguard.exe")
            .args(&["/uninstalltunnelservice", &self.interface_name])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| HvtError::Generic(format!("Failed to execute wireguard.exe: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);

            // If tunnel doesn't exist, consider it already down
            if stderr.contains("does not exist") || stderr.contains("not found") {
                return Ok(());
            }

            warn!("Failed to uninstall WireGuard tunnel: {}", stderr);
        }

        Ok(())
    }

    /// Check if wg-quick command is available (Unix)
    fn check_wg_quick_available(&self) -> Result<(), HvtError> {
        let output = Command::new("which")
            .arg("wg-quick")
            .output()
            .map_err(|e| HvtError::Generic(format!("Failed to check for wg-quick: {}", e)))?;

        if !output.status.success() {
            return Err(HvtError::Generic(
                "wg-quick not found. Please install WireGuard: sudo apt install wireguard-tools".to_string()
            ));
        }

        Ok(())
    }

    /// Check if wireguard.exe is available (Windows)
    fn check_wireguard_windows_available(&self) -> Result<(), HvtError> {
        let output = Command::new("where")
            .arg("wireguard.exe")
            .output()
            .map_err(|e| HvtError::Generic(format!("Failed to check for wireguard.exe: {}", e)))?;

        if !output.status.success() {
            return Err(HvtError::Generic(
                "wireguard.exe not found. Please install WireGuard from https://www.wireguard.com/install/".to_string()
            ));
        }

        Ok(())
    }

    /// Verify WireGuard connection is active
    fn verify_connection(&self) -> Result<(), HvtError> {
        if self.is_windows {
            // On Windows, check service status
            let output = Command::new("sc")
                .args(&["query", &format!("WireGuardTunnel${}", self.interface_name)])
                .output()
                .map_err(|e| HvtError::Generic(format!("Failed to verify WireGuard connection: {}", e)))?;

            if !output.status.success() {
                return Err(HvtError::Generic(format!(
                    "WireGuard tunnel {} not active",
                    self.interface_name
                )));
            }
        } else {
            // On Unix, use wg show
            let output = Command::new("sudo")
                .args(&["wg", "show", &self.interface_name])
                .output()
                .map_err(|e| HvtError::Generic(format!("Failed to verify WireGuard connection: {}", e)))?;

            if !output.status.success() {
                return Err(HvtError::Generic(format!(
                    "WireGuard interface {} not active",
                    self.interface_name
                )));
            }
        }

        Ok(())
    }

    /// Get connection status
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Test network connectivity through the VPN
    fn test_connectivity(&self) -> Result<(), HvtError> {
        // Try to ping a reliable DNS server through the VPN
        let output = if self.is_windows {
            // Windows ping syntax: ping -n 1 -w 5000 1.1.1.1
            Command::new("ping")
                .args(&["-n", "1", "-w", "5000", "1.1.1.1"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
        } else {
            // Unix ping syntax: ping -c 1 -W 5 1.1.1.1
            Command::new("ping")
                .args(&["-c", "1", "-W", "5", "1.1.1.1"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
        }
        .map_err(|e| HvtError::Generic(format!("Failed to test connectivity: {}", e)))?;

        if !output.status.success() {
            return Err(HvtError::Generic(
                "VPN connected but no network connectivity. Check your VPN configuration.".to_string()
            ));
        }

        info!("Network connectivity OK");
        Ok(())
    }
}

impl Drop for WireGuardManager {
    fn drop(&mut self) {
        // Automatically disconnect when the manager is dropped
        if self.connected {
            if let Err(e) = self.disconnect() {
                warn!("Failed to disconnect WireGuard on cleanup: {}", e);
            }
        }
    }
}
