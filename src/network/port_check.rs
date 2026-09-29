use std::time::Duration;
use anyhow::Result;
use colored::*;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio::time::timeout;

use super::ip::{get_network_info, NetworkInfo};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCheckReport {
    pub port: u16,
    pub is_listening_locally: bool,
    pub network_info: NetworkInfo,
    pub lan_address: Option<String>,
    pub wan_address: Option<String>,
    pub port_forwarding_status: String,
}

pub async fn check_local_port(port: u16) -> bool {
    let addr = format!("127.0.0.1:{}", port);
    match timeout(Duration::from_millis(500), TcpStream::connect(&addr)).await {
        Ok(Ok(_)) => true,
        _ => false,
    }
}

pub async fn check_external_port_reachable(public_ip: &str, port: u16) -> Result<bool> {
    // Check using a fast external port-check API
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    // Port check via portchecker / yougetsignal style check or mc status
    let check_url = format!("https://api.mcsrvstat.us/3/{}:{}", public_ip, port);
    if let Ok(res) = client.get(&check_url).send().await {
        if let Ok(json) = res.json::<serde_json::Value>().await {
            if let Some(online) = json.get("online").and_then(|v| v.as_bool()) {
                if online {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

pub async fn generate_diagnostics_report(port: u16) -> ServerCheckReport {
    let is_listening = check_local_port(port).await;
    let net = get_network_info().await;

    let lan_addr = net.local_ip.as_ref().map(|ip| format!("{}:{}", ip, port));
    let wan_addr = net.public_ip.as_ref().map(|ip| format!("{}:{}", ip, port));

    let mut pf_status = "Non verificabile (Server non attivo o verifica non riuscita)".to_string();
    if is_listening {
        if let Some(ref pub_ip) = net.public_ip {
            match check_external_port_reachable(pub_ip, port).await {
                Ok(true) => pf_status = "APERTA E RAGGIUNGIBILE DALL'ESTERNO (Port Forwarding OK)".to_string(),
                _ => pf_status = "In ascolto locale. Se giochi con amici fuori casa, verifica di aver aperto la porta 25565 TCP nel router (Port Forwarding).".to_string(),
            }
        }
    }

    ServerCheckReport {
        port,
        is_listening_locally: is_listening,
        network_info: net,
        lan_address: lan_addr,
        wan_address: wan_addr,
        port_forwarding_status: pf_status,
    }
}

pub fn print_diagnostics_report(report: &ServerCheckReport) {
    println!("{}", "═══════════════════════════════════════════════════════════".cyan());
    println!("{}", "       🔍 DIAGNOSTICA CONNETTIVITÀ SERVER MINECRAFT        ".cyan().bold());
    println!("{}", "═══════════════════════════════════════════════════════════".cyan());

    println!("• Porta Server: {}", report.port.to_string().yellow().bold());

    if report.is_listening_locally {
        println!("• Stato Processo Locale: {}", "✓ ATTIVO (In ascolto sulla porta)".green().bold());
    } else {
        println!("• Stato Processo Locale: {}", "✗ NON ATTIVO (Avvia il server con 'gsmm start')".red());
    }

    if let Some(ref lan) = report.lan_address {
        println!("• Indirizzo per la rete locale (LAN / Stessa casa): {}", lan.green().bold());
    }

    if let Some(ref wan) = report.wan_address {
        println!("\n• Indirizzo PUBBLICO (Da inviare agli amici online):");
        println!("  👉 {}", wan.magenta().bold());
    } else {
        println!("• Indirizzo PUBBLICO: {}", "Impossibile recuperare IP pubblico (Offline)".yellow());
    }

    println!("\n• Stato Port Forwarding:");
    println!("  ℹ {}", report.port_forwarding_status.cyan());
    println!("{}", "═══════════════════════════════════════════════════════════\n".cyan());
}
