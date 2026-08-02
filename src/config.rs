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
    /// DLSite calls go through an HTTP/SOCKS5 proxy instead of a locally-managed tunnel —
    /// there's no interface for hvtag to connect/disconnect itself. Intended for a deployment
    /// where a sidecar container (e.g. gluetun) already holds its own VPN tunnel up and exposes
    /// a proxy that only DLSite traffic is routed through; everything else (the web UI, remote
    /// folder pulls) never touches it.
    Proxy,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WireGuardConfig {
    /// Path to WireGuard configuration file (.conf)
    pub config_path: String,

    /// Optional interface name (defaults to config filename without extension)
    pub interface_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProxyConfig {
    /// e.g. "http://gluetun:8888" or "socks5://gluetun:1080"
    pub url: String,

    /// Optional proxy basic-auth credentials, if the proxy requires them.
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VpnConfig {
    /// Enable VPN functionality
    pub enabled: bool,

    /// VPN provider to use
    pub provider: VpnProvider,

    /// WireGuard-specific configuration
    pub wireguard: Option<WireGuardConfig>,

    /// Proxy configuration, used when `provider = "proxy"`
    pub proxy: Option<ProxyConfig>,
}

impl Default for VpnConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: VpnProvider::Wireguard,
            wireguard: None,
            proxy: None,
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

    /// Remote machines whose drop folders get rsync'd into `source_path` before each `--full`
    /// import — for the "several machines on the network deposit works, the server pulls them"
    /// deployment. Empty by default: nothing changes for anyone not using this.
    #[serde(default)]
    pub remote_sources: Vec<RemoteSource>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RemoteSource {
    /// Label used in progress/log output — purely cosmetic.
    pub name: String,

    pub host: String,

    #[serde(default = "default_ssh_port")]
    pub port: u16,

    pub user: String,

    /// Private key path for `ssh -i`. If omitted, ssh falls back to its own defaults
    /// (`~/.ssh/id_*`, ssh-agent, etc.) exactly as a plain `ssh` invocation would.
    pub ssh_key_path: Option<String>,

    /// Directory on the remote host whose *contents* (RJ/VJ folders) get pulled — not the
    /// directory itself.
    pub remote_path: String,

    /// After a successful pull, delete the source files on the remote host too (adds rsync's
    /// `--remove-source-files`). Off by default: a failed run leaves the remote copy intact to
    /// retry, and nothing gets deleted on a machine you don't have hvtag's eyes on without you
    /// opting in explicitly.
    #[serde(default)]
    pub remove_after_pull: bool,
}

fn default_ssh_port() -> u16 {
    22
}

// ========== Web UI Configuration ==========

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UiConfig {
    /// Bind address for the --ui web server. Defaults to loopback-only for safety.
    #[serde(default = "default_ui_bind_address")]
    pub bind_address: String,

    /// Port for the --ui web server.
    #[serde(default = "default_ui_port")]
    pub port: u16,

    /// Number of works shown per page in the works list.
    #[serde(default = "default_ui_page_size")]
    pub page_size: i64,
}

fn default_ui_bind_address() -> String {
    "127.0.0.1".to_string()
}

fn default_ui_port() -> u16 {
    8787
}

fn default_ui_page_size() -> i64 {
    50
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            bind_address: default_ui_bind_address(),
            port: default_ui_port(),
            page_size: default_ui_page_size(),
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

    #[serde(default)]
    pub import: ImportConfig,

    #[serde(default)]
    pub ui: UiConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            vpn: VpnConfig::default(),
            tagger: TaggerConfig::default(),
            import: ImportConfig::default(),
            ui: UiConfig::default(),
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

# Optional: machines on your network that deposit works into their own local drop folder,
# which `--full` pulls into source_path (via rsync over ssh) before scanning it. Repeat this
# block per machine. Requires the `rsync` and `ssh` binaries to be available.
# [[import.remote_sources]]
# name = "desktop-pc"
# host = "192.168.1.50"
# port = 22
# user = "eiko"
# ssh_key_path = "/home/eiko/.ssh/id_ed25519"  # optional; omit to use ssh's own defaults
# remote_path = "/home/eiko/hvtag-drop"
# remove_after_pull = false

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

# Alternative to [vpn.wireguard]: route DLSite calls through an HTTP/SOCKS5 proxy instead of a
# tunnel hvtag manages itself — for example a sidecar container (gluetun) that already has its
# own VPN connection up. Set provider = "proxy" above and uncomment below. Nothing else (the web
# UI, remote folder pulls) is routed through this proxy — only DLSite metadata/cover requests.
# [vpn.proxy]
# url = "http://gluetun:8888"
# username = "proxy-user"
# password = "proxy-pass"

[tagger]
# Use null byte separator (\0) for tags instead of custom separator
# Null separator is useful for certain media players that support it
use_null_separator = false

# Custom separator to use when use_null_separator is false
# Common separators: "; " (default), " / ", ", ", " | "
custom_separator = "; "

[ui]
# Bind address for the --ui web server. Defaults to loopback-only (127.0.0.1) for safety.
# To reach it from your phone over Tailscale/VPN, set this to your Tailscale IP
# (e.g. "100.x.y.z") or "0.0.0.0" to listen on all interfaces.
# SECURITY: hvtag's web UI has NO authentication. Only bind beyond 127.0.0.1
# if this machine's network exposure is fully controlled by your VPN/firewall -
# anyone who can reach this address and port gets full read+write access to your library.
bind_address = "127.0.0.1"

# Port for the --ui web server.
port = 8787

# Number of works shown per page in the works list.
page_size = 50
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
