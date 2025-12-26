use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;
use crate::errors::HvtError;

// ========== VPN Configuration ==========

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

impl Default for VpnConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: VpnProvider::Wireguard,
            wireguard: None,
        }
    }
}

// ========== Tagger Configuration ==========

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaggerConfig {
    /// Use null byte separator (\0) for tags instead of custom separator
    #[serde(default = "default_use_null_separator")]
    pub use_null_separator: bool,

    /// Custom separator to use when use_null_separator is false
    #[serde(default = "default_custom_separator")]
    pub custom_separator: String,
}

fn default_use_null_separator() -> bool {
    false
}

fn default_custom_separator() -> String {
    "; ".to_string()
}

impl Default for TaggerConfig {
    fn default() -> Self {
        Self {
            use_null_separator: false,
            custom_separator: "; ".to_string(),
        }
    }
}

impl TaggerConfig {
    /// Get the separator to use for joining tags
    pub fn get_separator(&self) -> String {
        if self.use_null_separator {
            "\0".to_string()
        } else {
            self.custom_separator.clone()
        }
    }
}

// ========== Root Configuration ==========

/// Root configuration structure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub vpn: VpnConfig,

    #[serde(default)]
    pub tagger: TaggerConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            vpn: VpnConfig::default(),
            tagger: TaggerConfig::default(),
        }
    }
}

impl Config {
    /// Load configuration from ~/.hvtag/config.toml
    pub fn load() -> Result<Self, HvtError> {
        let config_path = Self::get_config_path()?;

        if !config_path.exists() {
            // No config file, return default
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(&config_path)
            .map_err(|e| HvtError::Generic(format!("Failed to read config: {}", e)))?;

        let config: Config = toml::from_str(&contents)
            .map_err(|e| HvtError::Parse(format!("Failed to parse config: {}", e)))?;

        Ok(config)
    }

    /// Get the path to the configuration file
    fn get_config_path() -> Result<PathBuf, HvtError> {
        let home = std::env::var("HOME")
            .map_err(|_| HvtError::Generic("HOME environment variable not set".to_string()))?;

        let config_dir = PathBuf::from(home).join(".hvtag");

        // Create directory if it doesn't exist
        if !config_dir.exists() {
            std::fs::create_dir_all(&config_dir)
                .map_err(|e| HvtError::Generic(format!("Failed to create config directory: {}", e)))?;
        }

        Ok(config_dir.join("config.toml"))
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

        let sample = r#"# hvtag Configuration File

[vpn]
# Enable VPN functionality for metadata fetching
enabled = true
provider = "wireguard"

[vpn.wireguard]
# Path to your WireGuard configuration file
config_path = "/home/user/.hvtag/wg-japan.conf"

# Optional: custom interface name (defaults to config filename)
# interface_name = "wg-hvtag"

[tagger]
# Use null byte separator (\0) for tags instead of custom separator
# Null separator is useful for certain media players that support it
use_null_separator = false

# Custom separator to use when use_null_separator is false
# Common separators: "; " (default), " / ", ", ", " | "
custom_separator = "; "
"#;

        std::fs::write(&config_path, sample)
            .map_err(|e| HvtError::Generic(format!("Failed to write sample config: {}", e)))?;

        info!("Sample config created at: {}", config_path.display());
        Ok(())
    }
}
