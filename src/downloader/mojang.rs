use std::path::Path;
use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;

use super::download_file_with_progress;

const MOJANG_MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct MojangManifest {
    pub latest: MojangLatest,
    pub versions: Vec<MojangVersionEntry>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct MojangLatest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MojangVersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub release_type: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
struct VersionPackage {
    downloads: VersionDownloads,
}

#[derive(Debug, Deserialize)]
struct VersionDownloads {
    server: Option<DownloadFile>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct DownloadFile {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

pub struct MojangClient {
    client: Client,
}

impl MojangClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent("GSMM/0.1.0")
                .build()
                .unwrap_or_default(),
        }
    }

    pub async fn get_manifest(&self) -> Result<MojangManifest> {
        let manifest: MojangManifest = self
            .client
            .get(MOJANG_MANIFEST_URL)
            .send()
            .await?
            .json()
            .await
            .context("Impossibile parsare il manifest delle versioni Mojang")?;
        Ok(manifest)
    }

    pub async fn get_releases(&self) -> Result<Vec<String>> {
        let manifest = self.get_manifest().await?;
        let releases = manifest
            .versions
            .into_iter()
            .filter(|v| v.release_type == "release")
            .map(|v| v.id)
            .collect();
        Ok(releases)
    }

    pub async fn download_server(&self, version_id: &str, target_dir: &Path) -> Result<String> {
        let manifest = self.get_manifest().await?;
        let entry = manifest
            .versions
            .iter()
            .find(|v| v.id == version_id)
            .with_context(|| format!("Versione Minecraft '{}' non trovata nel manifest ufficiale Mojang!", version_id))?;

        let pkg: VersionPackage = self
            .client
            .get(&entry.url)
            .send()
            .await?
            .json()
            .await
            .context("Impossibile scaricare i metadati del pacchetto della versione")?;

        let server_download = pkg
            .downloads
            .server
            .with_context(|| format!("Nessun download di server disponibile per la versione '{}'", version_id))?;

        let jar_name = "server.jar";
        let target_path = target_dir.join(jar_name);

        println!("Scaricamento Vanilla Server {}...", version_id);
        download_file_with_progress(
            &self.client,
            &server_download.url,
            &target_path,
            &format!("Minecraft Vanilla {}", version_id),
        )
        .await?;

        Ok(jar_name.to_string())
    }
}
