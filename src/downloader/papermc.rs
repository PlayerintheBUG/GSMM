use std::path::Path;
use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;

use super::download_file_with_progress;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ProjectVersionsResponse {
    versions: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct VersionBuildsResponse {
    builds: Vec<u32>,
}

#[derive(Debug, Deserialize)]
struct BuildInfoResponse {
    downloads: BuildDownloads,
}

#[derive(Debug, Deserialize)]
struct BuildDownloads {
    application: BuildApplication,
}

#[derive(Debug, Deserialize)]
struct BuildApplication {
    name: String,
}

pub struct PaperClient {
    client: Client,
    project: String, // "paper" or "purpur" (purpur also has purpur api or paper)
}

impl PaperClient {
    pub fn new(project: &str) -> Self {
        Self {
            client: Client::builder().user_agent("GSMM/0.1.0").build().unwrap_or_default(),
            project: project.to_lowercase(),
        }
    }

    #[allow(dead_code)]
    pub async fn get_versions(&self) -> Result<Vec<String>> {
        let url = format!("https://api.papermc.io/v2/projects/{}", self.project);
        let resp: ProjectVersionsResponse = self
            .client
            .get(&url)
            .send()
            .await?
            .json()
            .await
            .with_context(|| format!("Errore nel recupero versioni da PaperMC per '{}'", self.project))?;

        let mut versions = resp.versions;
        versions.reverse(); // Most recent first
        Ok(versions)
    }

    pub async fn download_latest_build(&self, mc_version: &str, target_dir: &Path) -> Result<String> {
        let builds_url = format!(
            "https://api.papermc.io/v2/projects/{}/versions/{}",
            self.project, mc_version
        );

        let builds_resp: VersionBuildsResponse = self
            .client
            .get(&builds_url)
            .send()
            .await?
            .json()
            .await
            .with_context(|| format!("Nessuna build PaperMC trovata per {} {}", self.project, mc_version))?;

        let latest_build = builds_resp
            .builds
            .last()
            .with_context(|| format!("Lista build vuota per {} {}", self.project, mc_version))?;

        let build_info_url = format!(
            "https://api.papermc.io/v2/projects/{}/versions/{}/builds/{}",
            self.project, mc_version, latest_build
        );

        let build_info: BuildInfoResponse = self
            .client
            .get(&build_info_url)
            .send()
            .await?
            .json()
            .await
            .context("Impossibile recuperare i dettagli della build")?;

        let file_name = build_info.downloads.application.name;
        let download_url = format!(
            "https://api.papermc.io/v2/projects/{}/versions/{}/builds/{}/downloads/{}",
            self.project, mc_version, latest_build, file_name
        );

        let jar_name = "server.jar";
        let target_path = target_dir.join(jar_name);

        println!("Scaricamento {} {} (Build {})...", self.project, mc_version, latest_build);
        download_file_with_progress(
            &self.client,
            &download_url,
            &target_path,
            &format!("{} {} Build {}", self.project, mc_version, latest_build),
        )
        .await?;

        Ok(jar_name.to_string())
    }
}
