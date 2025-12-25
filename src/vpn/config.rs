use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;
use crate::errors::HvtError;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VpnProvider {
    Wireguard,
    ProtonVPN,
    OpenVPN,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WireGuardConfig {
    /// Path to WireGuard configuration file (.conf)
    pub config_path: String,

    /// Optional interface name (defaults to config filename without extension)
    pub interface_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VpnConfig {
    /// Enable VPN functionality
    pub enabled: bool,

    /// VPN provider to use
    pub provider: VpnProvider,

    /// WireGuard-specific configuration
    pub wireguard: Option<WireGuardConfig>,
}

/// Root configuration structure to match TOML format
#[derive(Debug, Clone, Deserialize, Serialize)]
struct VpnConfigFile {
    vpn: VpnConfig,
}

impl Default for VpnConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: VpnProvider::Wireguard,
            wireguard: None,
        }
    }
}

impl VpnConfig {
    /// Load VPN configuration from ~/.hvtag/vpn.toml
    pub fn load() -> Result<Self, HvtError> {
        let config_path = Self::get_config_path()?;

        if !config_path.exists() {
            // No config file, return default (disabled)
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(&config_path)
            .map_err(|e| HvtError::Generic(format!("Failed to read VPN config: {}", e)))?;

        let config_file: VpnConfigFile = toml::from_str(&contents)
            .map_err(|e| HvtError::Parse(format!("Failed to parse VPN config: {}", e)))?;

        Ok(config_file.vpn)
    }

    /// Get the path to the VPN configuration file
    fn get_config_path() -> Result<PathBuf, HvtError> {
        let home = std::env::var("HOME")
            .map_err(|_| HvtError::Generic("HOME environment variable not set".to_string()))?;

        let config_dir = PathBuf::from(home).join(".hvtag");

        // Create directory if it doesn't exist
        if !config_dir.exists() {
            std::fs::create_dir_all(&config_dir)
                .map_err(|e| HvtError::Generic(format!("Failed to create config directory: {}", e)))?;
        }

        Ok(config_dir.join("vpn.toml"))
    }

    /// Create a sample configuration file
    pub fn create_sample() -> Result<(), HvtError> {
        let config_path = Self::get_config_path()?;

        if config_path.exists() {
            return Err(HvtError::Generic(format!(
                "Config file already exists at {}",
                config_path.display()
            )));
        }

        let sample = r#"# hvtag VPN Configuration
# This file enables automatic VPN connection when fetching metadata from DLSite

[vpn]
enabled = true
provider = "wireguard"

[vpn.wireguard]
# Path to your WireGuard configuration file
config_path = "/home/user/.hvtag/wg-japan.conf"

# Optional: custom interface name (defaults to config filename)
# interface_name = "wg-hvtag"
"#;

        std::fs::write(&config_path, sample)
            .map_err(|e| HvtError::Generic(format!("Failed to write sample config: {}", e)))?;

        info!("Sample VPN config created at: {}", config_path.display());
        Ok(())
    }
}
