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

// ========== Import Configuration ==========

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ImportConfig {
    /// Source directory where new works are dropped for import
    pub source_path: Option<String>,

    /// Target library directory where works are moved after processing
    pub library_path: Option<String>,
}

// ========== Root Configuration ==========

/// Root configuration structure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub vpn: VpnConfig,

    #[serde(default)]
    pub tagger: TaggerConfig,

    #[serde(default)]
    pub import: ImportConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            vpn: VpnConfig::default(),
            tagger: TaggerConfig::default(),
            import: ImportConfig::default(),
        }
    }
}

impl Config {
    /// Load configuration from ~/.hvtag/config.toml
    /// Creates a default config file if it doesn't exist
    pub fn load() -> Result<Self, HvtError> {
        let config_path = Self::get_config_path()?;

        if !config_path.exists() {
            // Create default config file for new users
            info!("No config file found, creating default at: {}", config_path.display());
            Self::create_default_config(&config_path)?;
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(&config_path)
            .map_err(|e| HvtError::Generic(format!("Failed to read config: {}", e)))?;

        let config: Config = toml::from_str(&contents)
            .map_err(|e| HvtError::Parse(format!("Failed to parse config: {}", e)))?;

        Ok(config)
    }

    /// Create a default configuration file
    fn create_default_config(config_path: &PathBuf) -> Result<(), HvtError> {
        let default_config = Self::get_default_config_content();

        std::fs::write(config_path, default_config)
            .map_err(|e| HvtError::Generic(format!("Failed to write default config: {}", e)))?;

        Ok(())
    }

    /// Get the default configuration content with platform-specific paths
    fn get_default_config_content() -> String {
        let (wg_example, source_example, library_example) = if cfg!(target_os = "windows") {
            (
                "C:\\\\Users\\\\<username>\\\\.hvtag\\\\wireguard.conf",
                "D:\\\\Downloads\\\\ASMR",
                "E:\\\\Library\\\\ASMR",
            )
        } else {
            (
                "/home/<username>/.hvtag/wireguard.conf",
                "/home/<username>/Downloads/ASMR",
                "/home/<username>/Library/ASMR",
            )
        };

        format!(r#"# hvtag Configuration File
# Edit this file to customize hvtag behavior

[import]
# Source directory: where new works are dropped for import
# source_path = "{source_example}"

# Library directory: where works are moved after processing
# library_path = "{library_example}"

[vpn]
# Enable VPN functionality for metadata fetching from DLsite
# Set to true if you need to access DLsite from a restricted region
enabled = false
provider = "wireguard"

[vpn.wireguard]
# Path to your WireGuard configuration file (.conf)
# Replace with your actual WireGuard config file path
config_path = "{wg_example}"

# Optional: custom interface name (defaults to config filename without extension)
# interface_name = "wg-hvtag"

[tagger]
# Use null byte separator (\0) for tags instead of custom separator
# Null separator is useful for certain media players that support it
use_null_separator = false

# Custom separator to use when use_null_separator is false
# Common separators: "; " (default), " / ", ", ", " | "
custom_separator = "; "
"#)
    }

    /// Get the path to the configuration file
    fn get_config_path() -> Result<PathBuf, HvtError> {
        let home = dirs::home_dir()
            .ok_or_else(|| HvtError::Generic("Could not determine home directory".to_string()))?;

        let config_dir = home.join(".hvtag");

        // Create directory if it doesn't exist
        if !config_dir.exists() {
            std::fs::create_dir_all(&config_dir)
                .map_err(|e| HvtError::Generic(format!("Failed to create config directory: {}", e)))?;
        }

        Ok(config_dir.join("config.toml"))
    }

}
