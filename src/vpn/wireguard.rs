use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tracing::{debug, info, warn};
use crate::errors::HvtError;
use crate::config::WireGuardConfig;

/// Default WireGuard installation path on Windows
const WIREGUARD_WINDOWS_PATH: &str = "C:\\Program Files\\WireGuard";

pub struct WireGuardManager {
    interface_name: String,
    config_path: String,
    connected: bool,
    /// True if WE initiated the connection (vs reusing existing)
    we_initiated_connection: bool,
    is_windows: bool,
    /// Path to wireguard.exe on Windows
    wireguard_exe: Option<PathBuf>,
    /// Path to wg.exe on Windows
    wg_exe: Option<PathBuf>,
}

impl WireGuardManager {
    /// Create a new WireGuard manager from configuration
    pub fn new(config: &WireGuardConfig) -> Result<Self, HvtError> {
        let config_path = config.config_path.clone();
        let is_windows = cfg!(target_os = "windows");

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

        // Find WireGuard executables on Windows
        let (wireguard_exe, wg_exe) = if is_windows {
            let wg_dir = PathBuf::from(WIREGUARD_WINDOWS_PATH);
            let wireguard = wg_dir.join("wireguard.exe");
            let wg = wg_dir.join("wg.exe");

            if !wireguard.exists() {
                return Err(HvtError::Generic(format!(
                    "WireGuard not found at {}. Please install WireGuard from https://www.wireguard.com/install/",
                    wireguard.display()
                )));
            }

            (Some(wireguard), Some(wg))
        } else {
            (None, None)
        };

        Ok(Self {
            interface_name,
            config_path,
            connected: false,
            we_initiated_connection: false,
            is_windows,
            wireguard_exe,
            wg_exe,
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
            self.we_initiated_connection = false; // Don't disconnect on drop
            return Ok(());
        }

        // Platform-specific connection
        if self.is_windows {
            self.connect_windows()?;
        } else {
            self.connect_unix()?;
        }

        self.connected = true;
        self.we_initiated_connection = true; // We created this connection, so disconnect on drop

        // Verify connection (with retries on Windows)
        debug!("Verifying WireGuard connection...");
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
        let wireguard_exe = self.wireguard_exe.as_ref()
            .ok_or_else(|| HvtError::Generic("WireGuard executable path not set".to_string()))?;

        debug!("Using WireGuard at: {}", wireguard_exe.display());

        // On Windows, use: wireguard.exe /installtunnelservice <config-path>
        let output = Command::new(wireguard_exe)
            .args(&["/installtunnelservice", &self.config_path])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| HvtError::Generic(format!("Failed to execute wireguard.exe: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            // Check if tunnel is already installed
            if stderr.contains("already exists") || stderr.contains("already installed")
               || stdout.contains("already exists") {
                info!("WireGuard tunnel {} already active", self.interface_name);
                return Ok(());
            }

            // Log full output for debugging
            debug!("wireguard.exe stdout: {}", stdout);
            debug!("wireguard.exe stderr: {}", stderr);

            // Check for permission error
            if stderr.contains("Access is denied") || stdout.contains("Access is denied") {
                return Err(HvtError::Generic(
                    "WireGuard requires administrator privileges. Please run the program as Administrator.".to_string()
                ));
            }

            return Err(HvtError::Generic(format!(
                "Failed to install WireGuard tunnel: {} {}",
                stdout, stderr
            )));
        }

        Ok(())
    }

    /// Check if the WireGuard interface already exists
    pub fn interface_exists(&self) -> Result<bool, HvtError> {
        if self.is_windows {
            // On Windows, use wg.exe show to check interface
            if let Some(wg_exe) = &self.wg_exe {
                let output = Command::new(wg_exe)
                    .args(&["show", &self.interface_name])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .map_err(|e| HvtError::Generic(format!("Failed to check interface: {}", e)))?;

                Ok(output.status.success())
            } else {
                // Fallback: check if tunnel service is running
                let output = Command::new("sc")
                    .args(&["query", &format!("WireGuardTunnel${}", self.interface_name)])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .map_err(|e| HvtError::Generic(format!("Failed to check interface: {}", e)))?;

                Ok(output.status.success())
            }
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
        let wireguard_exe = self.wireguard_exe.as_ref()
            .ok_or_else(|| HvtError::Generic("WireGuard executable path not set".to_string()))?;

        let output = Command::new(wireguard_exe)
            .args(&["/uninstalltunnelservice", &self.interface_name])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| HvtError::Generic(format!("Failed to execute wireguard.exe: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            // If tunnel doesn't exist, consider it already down
            if stderr.contains("does not exist") || stderr.contains("not found")
               || stdout.contains("does not exist") {
                return Ok(());
            }

            warn!("Failed to uninstall WireGuard tunnel: {} {}", stdout, stderr);
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
                "wg-quick not found. Please install WireGuard tools for your system.".to_string()
            ));
        }

        Ok(())
    }

    /// Verify WireGuard connection is active
    fn verify_connection(&self) -> Result<(), HvtError> {
        if self.is_windows {
            // On Windows, retry verification as service may take time to start
            let max_retries = 10;
            let retry_delay = std::time::Duration::from_secs(2);

            for attempt in 1..=max_retries {
                let is_active = if let Some(wg_exe) = &self.wg_exe {
                    let output = Command::new(wg_exe)
                        .args(&["show", &self.interface_name])
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .output()
                        .map_err(|e| HvtError::Generic(format!("Failed to verify WireGuard connection: {}", e)))?;
                    output.status.success()
                } else {
                    // Fallback: check service status
                    let output = Command::new("sc")
                        .args(&["query", &format!("WireGuardTunnel${}", self.interface_name)])
                        .output()
                        .map_err(|e| HvtError::Generic(format!("Failed to verify WireGuard connection: {}", e)))?;
                    output.status.success()
                };

                if is_active {
                    debug!("WireGuard tunnel verified on attempt {}", attempt);
                    return Ok(());
                }

                if attempt < max_retries {
                    debug!("WireGuard tunnel not ready (attempt {}/{}), retrying...", attempt, max_retries);
                    std::thread::sleep(retry_delay);
                }
            }

            return Err(HvtError::Generic(format!(
                "WireGuard tunnel {} not active after {} attempts",
                self.interface_name, max_retries
            )));
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
        // Only disconnect if WE initiated the connection
        // Don't disconnect if we were reusing an existing VPN connection
        if self.connected && self.we_initiated_connection {
            info!("Disconnecting WireGuard (initiated by this session)...");
            if let Err(e) = self.disconnect() {
                warn!("Failed to disconnect WireGuard on cleanup: {}", e);
            }
        } else if self.connected {
            debug!("Keeping VPN connected (was already active before this session)");
        }
    }
}
