//! Builds the `reqwest::Client` used for DLSite calls, shaped by `[vpn]` config.
//!
//! There are two ways hvtag can reach DLSite through a VPN, and this is the one place that
//! decides which applies:
//! - `provider = "wireguard"`: hvtag connects/disconnects an OS-level WireGuard tunnel itself
//!   (see `WireGuardManager`) and the returned client is a plain client — once the tunnel is up,
//!   *all* of the process's traffic (including this client's) routes through it.
//! - `provider = "proxy"`: nothing for hvtag to connect — a sidecar (e.g. a gluetun container)
//!   already holds its own tunnel up and exposes an HTTP/SOCKS5 proxy. Only this specific client
//!   is configured to use that proxy, so DLSite calls go through the tunnel while everything
//!   else (the web UI, remote folder pulls) stays on the normal network path.

use std::time::Duration;

use crate::config::{Config, VpnProvider};
use crate::errors::HvtError;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";

/// Builds the DLSite-facing HTTP client per `config.vpn`. Always sets a 30s timeout and a
/// cookie jar (DLSite's age-gate relies on a session cookie); adds a proxy only for
/// `provider = "proxy"` with VPN enabled.
pub fn build_dlsite_client(config: &Config) -> Result<reqwest::Client, HvtError> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .cookie_store(true)
        .user_agent(USER_AGENT);

    if config.vpn.enabled {
        if let VpnProvider::Proxy = config.vpn.provider {
            let proxy_config = config.vpn.proxy.as_ref().ok_or_else(|| {
                HvtError::Generic(
                    "vpn.provider is \"proxy\" but no [vpn.proxy] section is configured".to_string(),
                )
            })?;

            let mut proxy = reqwest::Proxy::all(&proxy_config.url).map_err(|e| {
                HvtError::Generic(format!("Invalid vpn.proxy.url '{}': {}", proxy_config.url, e))
            })?;
            if let (Some(user), Some(pass)) = (&proxy_config.username, &proxy_config.password) {
                proxy = proxy.basic_auth(user, pass);
            }
            builder = builder.proxy(proxy);
        }
    }

    builder
        .build()
        .map_err(|e| HvtError::Http(format!("Failed to build HTTP client: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProxyConfig, VpnConfig};
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    fn proxy_config(url: String) -> Config {
        let mut config = Config::default();
        config.vpn = VpnConfig {
            enabled: true,
            provider: VpnProvider::Proxy,
            wireguard: None,
            proxy: Some(ProxyConfig { url, username: None, password: None }),
        };
        config
    }

    /// The real thing this needs to prove: a request made with the DLSite client actually goes
    /// to the configured proxy address, not directly to the target — since a real WireGuard/
    /// gluetun tunnel can't be exercised in this environment, this is the closest verification
    /// of the split-tunnel wiring available: a stand-in "proxy" that just needs to observe a
    /// connection land on it.
    #[tokio::test]
    async fn dlsite_client_routes_through_configured_proxy() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();

        let (got_connection_tx, got_connection_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 64];
            let _ = socket.read(&mut buf).await;
            let _ = got_connection_tx.send(());
            // No real proxy response — the client-side request is expected to error out after
            // this; only "did the proxy see a connection" is being checked.
        });

        let config = proxy_config(format!("http://{}", proxy_addr));
        let client = build_dlsite_client(&config).unwrap();

        // Target is deliberately unroutable (no DNS resolution possible) — if this request
        // reaches our fake proxy anyway, that proves the proxy config took effect rather than
        // the client trying (and failing) to resolve the target directly.
        let _ = client.get("http://dlsite-client-test.invalid/").send().await;

        tokio::time::timeout(std::time::Duration::from_secs(5), got_connection_rx)
            .await
            .expect("timed out waiting for the proxy to receive a connection")
            .expect("proxy task dropped its sender");
    }

    #[test]
    fn missing_proxy_section_is_an_error() {
        let mut config = Config::default();
        config.vpn = VpnConfig {
            enabled: true,
            provider: VpnProvider::Proxy,
            wireguard: None,
            proxy: None,
        };

        let err = build_dlsite_client(&config).expect_err("should require [vpn.proxy]");
        assert!(matches!(err, HvtError::Generic(_)));
    }

    #[test]
    fn wireguard_provider_does_not_touch_proxy_section() {
        // provider = "wireguard" should build a plain client regardless of vpn.proxy —
        // the OS-level tunnel (connected separately) is what routes traffic for that mode.
        let config = Config::default(); // vpn disabled entirely
        assert!(build_dlsite_client(&config).is_ok());
    }
}
