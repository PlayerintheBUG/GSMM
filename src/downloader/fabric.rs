use std::path::Path;
use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;

use super::download_file_with_progress;

const FABRIC_META_URL: &str = "https://meta.fabricmc.net/v2";

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct FabricGameVersion {
    pub version: String,
    pub stable: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FabricLoaderVersion {
    pub version: String,
    pub stable: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FabricInstallerVersion {
    pub version: String,
    pub stable: bool,
}

pub struct FabricClient {
    client: Client,
}

impl FabricClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder().user_agent("GSMM/0.1.0").build().unwrap_or_default(),
        }
    }

    #[allow(dead_code)]
    pub async fn get_game_versions(&self) -> Result<Vec<String>> {
        let url = format!("{}/versions/game", FABRIC_META_URL);
        let list: Vec<FabricGameVersion> = self
            .client
            .get(&url)
            .send()
            .await?
            .json()
            .await
            .context("Impossibile recuperare versioni Minecraft da Fabric Meta")?;

        let versions = list.into_iter().filter(|v| v.stable).map(|v| v.version).collect();
        Ok(versions)
    }

    pub async fn get_latest_loader_version(&self) -> Result<String> {
        let url = format!("{}/versions/loader", FABRIC_META_URL);
        let list: Vec<FabricLoaderVersion> = self
            .client
            .get(&url)
            .send()
            .await?
            .json()
            .await
            .context("Impossibile recuperare versioni Loader da Fabric Meta")?;

        let latest = list
            .iter()
            .find(|l| l.stable)
            .or_else(|| list.first())
            .with_context(|| "Nessuna versione loader Fabric disponibile")?;

        Ok(latest.version.clone())
    }

    pub async fn get_latest_installer_version(&self) -> Result<String> {
        let url = format!("{}/versions/installer", FABRIC_META_URL);
        let list: Vec<FabricInstallerVersion> = self
            .client
            .get(&url)
            .send()
            .await?
            .json()
            .await
            .context("Impossibile recuperare versioni Installer da Fabric Meta")?;

        let latest = list
            .iter()
            .find(|i| i.stable)
            .or_else(|| list.first())
            .with_context(|| "Nessuna versione installer Fabric disponibile")?;

        Ok(latest.version.clone())
    }

    pub async fn download_server(&self, mc_version: &str, target_dir: &Path) -> Result<String> {
        let loader_version = self.get_latest_loader_version().await?;
        let installer_version = self.get_latest_installer_version().await?;

        let download_url = format!(
            "{}/versions/loader/{}/{}/{}/server/jar",
            FABRIC_META_URL, mc_version, loader_version, installer_version
        );

        let jar_name = "server.jar";
        let target_path = target_dir.join(jar_name);

        println!("Scaricamento Fabric Server {} (Loader {}, Installer {})...", mc_version, loader_version, installer_version);
        download_file_with_progress(
            &self.client,
            &download_url,
            &target_path,
            &format!("Fabric Server {}", mc_version),
        )
        .await?;

        Ok(jar_name.to_string())
    }
}
