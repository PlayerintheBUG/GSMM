use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use anyhow::{Context, Result};
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LoaderType {
    Vanilla,
    Paper,
    Purpur,
    Fabric,
    NeoForge,
    Forge,
}

impl std::fmt::Display for LoaderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoaderType::Vanilla => write!(f, "Vanilla"),
            LoaderType::Paper => write!(f, "Paper"),
            LoaderType::Purpur => write!(f, "Purpur"),
            LoaderType::Fabric => write!(f, "Fabric"),
            LoaderType::NeoForge => write!(f, "NeoForge"),
            LoaderType::Forge => write!(f, "Forge"),
        }
    }
}

impl std::str::FromStr for LoaderType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "vanilla" => Ok(LoaderType::Vanilla),
            "paper" => Ok(LoaderType::Paper),
            "purpur" => Ok(LoaderType::Purpur),
            "fabric" => Ok(LoaderType::Fabric),
            "neoforge" | "neo" => Ok(LoaderType::NeoForge),
            "forge" => Ok(LoaderType::Forge),
            other => anyhow::bail!("Loader sconosciuto: {}", other),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub name: String,
    pub mc_version: String,
    pub loader: LoaderType,
    pub loader_version: Option<String>,
    pub server_jar: String,
    pub java_path: Option<String>,
    pub ram_min_mb: u32,
    pub ram_max_mb: u32,
    pub port: u16,
    pub jvm_args: Vec<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            name: "Minecraft Server".to_string(),
            mc_version: "1.21.1".to_string(),
            loader: LoaderType::Fabric,
            loader_version: None,
            server_jar: "server.jar".to_string(),
            java_path: None,
            ram_min_mb: 2048,
            ram_max_mb: 4096,
            port: 25565,
            jvm_args: vec![
                "-XX:+UseG1GC".to_string(),
                "-XX:+ParallelRefProcEnabled".to_string(),
                "-XX:MaxGCPauseMillis=200".to_string(),
                "-XX:+UnlockExperimentalVMOptions".to_string(),
                "-XX:+DisableExplicitGC".to_string(),
                "-XX:+AlwaysPreTouch".to_string(),
            ],
        }
    }
}

pub const CONFIG_FILE_NAME: &str = ".gsmm.json";

impl ServerConfig {
    pub async fn load_from_dir(dir: &Path) -> Result<Self> {
        let config_path = dir.join(CONFIG_FILE_NAME);
        if !config_path.exists() {
            anyhow::bail!("Nessuna configurazione GSMM trovata in '{}'. Esegui prima 'gsmm init'!", dir.display());
        }
        let content = fs::read_to_string(&config_path)
            .await
            .with_context(|| format!("Impossibile leggere '{}'", config_path.display()))?;
        let config: ServerConfig = serde_json::from_str(&content)
            .with_context(|| format!("Errore nel parsing di '{}'", config_path.display()))?;
        Ok(config)
    }

    pub async fn save_to_dir(&self, dir: &Path) -> Result<PathBuf> {
        let config_path = dir.join(CONFIG_FILE_NAME);
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&config_path, content)
            .await
            .with_context(|| format!("Impossibile salvare '{}'", config_path.display()))?;
        Ok(config_path)
    }

    pub async fn ensure_eula(dir: &Path) -> Result<()> {
        let eula_path = dir.join("eula.txt");
        let content = "# By changing the setting below to TRUE you are indicating your agreement to the Mojang EULA (https://account.mojang.com/documents/minecraft_eula).\neula=true\n";
        fs::write(&eula_path, content)
            .await
            .with_context(|| format!("Impossibile creare '{}'", eula_path.display()))?;
        Ok(())
    }

    pub async fn ensure_server_properties(dir: &Path, port: u16, motd: &str) -> Result<()> {
        let prop_path = dir.join("server.properties");
        if !prop_path.exists() {
            let content = format!(
                "server-port={port}\nquery.port={port}\nmotd={motd}\nonline-mode=true\nenable-rcon=false\nmax-players=20\nview-distance=10\nsimulation-distance=8\nsync-chunk-writes=true\n"
            );
            fs::write(&prop_path, content)
                .await
                .with_context(|| format!("Impossibile creare '{}'", prop_path.display()))?;
        }
        Ok(())
    }
}
