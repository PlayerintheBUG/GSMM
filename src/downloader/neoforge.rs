use std::path::Path;
use anyhow::{Context, Result};
use reqwest::Client;
use std::process::Command;

use super::download_file_with_progress;
use crate::core::java::detect_java;

pub struct NeoForgeClient {
    client: Client,
}

impl NeoForgeClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder().user_agent("GSMM/0.1.0").build().unwrap_or_default(),
        }
    }

    pub async fn download_and_install(&self, mc_version: &str, target_dir: &Path, java_path: Option<&str>) -> Result<String> {
        let java = detect_java(java_path)?;

        // Query neoforge maven metadata to get latest version for mc_version
        // NeoForge versioning mapping: MC 1.21.1 -> 21.1.x, MC 1.20.4 -> 20.4.x
        let neoforge_prefix = if let Some(stripped) = mc_version.strip_prefix("1.") {
            stripped.to_string()
        } else {
            mc_version.to_string()
        };

        println!("Ricerca versione NeoForge per MC {}...", mc_version);
        let metadata_url = "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";
        let xml_res = self.client.get(metadata_url).send().await?.text().await?;

        // Find latest version matching prefix
        let re = regex::Regex::new(r"<version>([^<]+)</version>")?;
        let mut matched_versions: Vec<String> = re
            .captures_iter(&xml_res)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .filter(|v| v.starts_with(&neoforge_prefix))
            .collect();

        if matched_versions.is_empty() {
            anyhow::bail!("Nessuna versione NeoForge trovata per Minecraft {}", mc_version);
        }

        let latest_neo_version = matched_versions.pop().unwrap();
        let installer_url = format!(
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/{}/neoforge-{}-installer.jar",
            latest_neo_version, latest_neo_version
        );

        let installer_path = target_dir.join("neoforge-installer.jar");
        download_file_with_progress(
            &self.client,
            &installer_url,
            &installer_path,
            &format!("NeoForge {} Installer", latest_neo_version),
        )
        .await?;

        println!("Esecuzione dell'installer NeoForge headless...");
        let status = Command::new(&java.path)
            .current_dir(target_dir)
            .arg("-jar")
            .arg("neoforge-installer.jar")
            .arg("--installServer")
            .status()
            .with_context(|| "Errore durante l'esecuzione dell'installer NeoForge")?;

        if !status.success() {
            anyhow::bail!("L'installer NeoForge ha restituito un codice di errore");
        }

        // Clean up installer jar
        let _ = tokio::fs::remove_file(&installer_path).await;
        let _ = tokio::fs::remove_file(target_dir.join("neoforge-installer.jar.log")).await;

        // Check launch script / jar
        #[cfg(target_os = "windows")]
        let launcher = "run.bat";
        #[cfg(not(target_os = "windows"))]
        let launcher = "run.sh";

        if target_dir.join(launcher).exists() {
            #[cfg(not(target_os = "windows"))]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = std::fs::metadata(target_dir.join(launcher)) {
                    let mut perms = metadata.permissions();
                    perms.set_mode(0o755);
                    let _ = std::fs::set_permissions(target_dir.join(launcher), perms);
                }
            }
        }

        // Look for generated server jar or use user_jvm_args
        let jar_name = "server.jar";
        Ok(jar_name.to_string())
    }
}
