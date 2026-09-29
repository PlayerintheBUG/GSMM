use std::collections::HashSet;
use std::path::Path;
use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::download_file_with_progress;

const MODRINTH_API_URL: &str = "https://api.modrinth.com/v2";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthSearchResult {
    pub hits: Vec<ModrinthSearchHit>,
    pub total_hits: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthSearchHit {
    pub slug: String,
    pub title: String,
    pub description: String,
    pub client_side: String,
    pub server_side: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthVersion {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub files: Vec<ModrinthFile>,
    pub dependencies: Vec<ModrinthDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthFile {
    pub url: String,
    pub filename: String,
    pub primary: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthDependency {
    pub version_id: Option<String>,
    pub project_id: Option<String>,
    pub dependency_type: String, // "required", "optional", "incompatible"
}

pub struct ModrinthClient {
    client: Client,
}

impl ModrinthClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent("GSMM/0.1.0 (https://github.com/mattia/GSMM)")
                .build()
                .unwrap_or_default(),
        }
    }

    pub async fn search_mods(
        &self,
        query: &str,
        mc_version: Option<&str>,
        loader: Option<&str>,
        limit: u32,
    ) -> Result<Vec<ModrinthSearchHit>> {
        let mut facets = vec![r#"["project_type:mod"]"#.to_string()];

        if let Some(l) = loader {
            facets.push(format!(r#"["categories:{}"]"#, l.to_lowercase()));
        }
        if let Some(v) = mc_version {
            facets.push(format!(r#"["versions:{}"]"#, v));
        }

        let facets_json = format!("[{}]", facets.join(","));
        let url = format!("{}/search", MODRINTH_API_URL);

        let res: ModrinthSearchResult = self
            .client
            .get(&url)
            .query(&[
                ("query", query),
                ("facets", &facets_json),
                ("limit", &limit.to_string()),
            ])
            .send()
            .await?
            .json()
            .await
            .context("Impossibile effettuare la ricerca su Modrinth")?;

        Ok(res.hits)
    }

    pub async fn get_version_by_id(&self, version_id: &str) -> Result<ModrinthVersion> {
        let url = format!("{}/version/{}", MODRINTH_API_URL, version_id);
        let res = self.client.get(&url).send().await?;
        if !res.status().is_success() {
            anyhow::bail!("Versione ID '{}' non trovata su Modrinth: {}", version_id, res.status());
        }
        let version: ModrinthVersion = res.json().await?;
        Ok(version)
    }

    pub async fn get_compatible_version(
        &self,
        project_slug_or_id: &str,
        mc_version: &str,
        loader: &str,
    ) -> Result<ModrinthVersion> {
        let url = format!("{}/project/{}/version", MODRINTH_API_URL, project_slug_or_id);
        let game_versions_json = serde_json::to_string(&vec![mc_version])?;
        let loaders_json = serde_json::to_string(&vec![loader.to_lowercase()])?;

        let res = self
            .client
            .get(&url)
            .query(&[
                ("game_versions", &game_versions_json),
                ("loaders", &loaders_json),
            ])
            .send()
            .await
            .with_context(|| format!("Errore chiamata Modrinth per '{}'", project_slug_or_id))?;

        if !res.status().is_success() {
            anyhow::bail!("Mod '{}' non trovata su Modrinth o errore API: {}", project_slug_or_id, res.status());
        }

        let versions: Vec<ModrinthVersion> = res
            .json()
            .await
            .with_context(|| format!("Errore nel parsing delle versioni per '{}'", project_slug_or_id))?;

        let version = versions
            .into_iter()
            .next()
            .with_context(|| {
                format!(
                    "Nessuna versione di '{}' compatibile trovata per Minecraft {} e loader {}",
                    project_slug_or_id, mc_version, loader
                )
            })?;

        Ok(version)
    }

    pub async fn install_mod_with_dependencies(
        &self,
        slug_or_id: &str,
        mc_version: &str,
        loader: &str,
        mods_dir: &Path,
        installed_projects: &mut HashSet<String>,
    ) -> Result<Vec<String>> {
        let clean_slug = slug_or_id.trim().trim_matches('"').trim_matches('\'').to_lowercase();
        if installed_projects.contains(&clean_slug) {
            return Ok(Vec::new());
        }

        let version = self.get_compatible_version(&clean_slug, mc_version, loader).await?;
        self.install_version_files(&version, mc_version, loader, mods_dir, installed_projects).await
    }

    pub async fn install_version_files(
        &self,
        version: &ModrinthVersion,
        mc_version: &str,
        loader: &str,
        mods_dir: &Path,
        installed_projects: &mut HashSet<String>,
    ) -> Result<Vec<String>> {
        if installed_projects.contains(&version.project_id) || installed_projects.contains(&version.id) {
            return Ok(Vec::new());
        }

        // Primary file or first file
        let file = version
            .files
            .iter()
            .find(|f| f.primary)
            .or_else(|| version.files.first())
            .with_context(|| format!("Nessun file scaricabile trovato per la versione {}", version.name))?;

        tokio::fs::create_dir_all(mods_dir).await?;
        let target_path = mods_dir.join(&file.filename);

        println!("Installazione mod [{}]...", file.filename);
        download_file_with_progress(&self.client, &file.url, &target_path, &file.filename).await?;
        installed_projects.insert(version.project_id.clone());
        installed_projects.insert(version.id.clone());

        let mut downloaded_files = vec![file.filename.clone()];

        // Check required dependencies
        for dep in &version.dependencies {
            if dep.dependency_type == "required" {
                if let Some(ref ver_id) = dep.version_id {
                    if !installed_projects.contains(ver_id) {
                        if let Ok(dep_ver) = self.get_version_by_id(ver_id).await {
                            println!("↳ Trovata dipendenza richiesta per versione: {}", dep_ver.name);
                            if let Ok(mut dep_files) = Box::pin(self.install_version_files(
                                &dep_ver,
                                mc_version,
                                loader,
                                mods_dir,
                                installed_projects,
                            )).await {
                                downloaded_files.append(&mut dep_files);
                                continue;
                            }
                        }
                    }
                }

                if let Some(ref proj_id) = dep.project_id {
                    if !installed_projects.contains(proj_id) {
                        println!("↳ Trovata dipendenza richiesta: {}", proj_id);
                        if let Ok(mut dep_files) = Box::pin(self.install_mod_with_dependencies(
                            proj_id,
                            mc_version,
                            loader,
                            mods_dir,
                            installed_projects,
                        )).await {
                            downloaded_files.append(&mut dep_files);
                        }
                    }
                }
            }
        }

        Ok(downloaded_files)
    }

    pub async fn list_installed_mods(mods_dir: &Path) -> Result<Vec<String>> {
        let mut list = Vec::new();
        if !mods_dir.exists() {
            return Ok(list);
        }
        let mut entries = tokio::fs::read_dir(mods_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "jar" {
                        if let Some(name) = path.file_name() {
                            list.push(name.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
        list.sort();
        Ok(list)
    }
}
