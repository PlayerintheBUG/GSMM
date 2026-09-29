use std::net::UdpSocket;
use anyhow::{Context, Result};
use reqwest::Client;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NetworkInfo {
    pub local_ip: Option<String>,
    pub public_ip: Option<String>,
}

pub fn get_local_ip() -> Option<String> {
    // Connect to a public DNS IP (doesn't actually send network packets)
    // to discover the local IP of the default outbound network interface
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let local_addr = socket.local_addr().ok()?;
    Some(local_addr.ip().to_string())
}

pub async fn get_public_ip() -> Result<String> {
    let client = Client::builder()
        .user_agent("GSMM/0.1.0")
        .timeout(std::time::Duration::from_secs(4))
        .build()
        .unwrap_or_default();

    // Try primary provider
    if let Ok(res) = client.get("https://api.ipify.org").send().await {
        if let Ok(text) = res.text().await {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
    }

    // Try fallback provider
    let res = client
        .get("https://icanhazip.com")
        .send()
        .await
        .context("Impossibile determinare l'IP pubblico")?;

    let text = res.text().await?;
    Ok(text.trim().to_string())
}

pub async fn get_network_info() -> NetworkInfo {
    let local_ip = get_local_ip();
    let public_ip = get_public_ip().await.ok();
    NetworkInfo {
        local_ip,
        public_ip,
    }
}
