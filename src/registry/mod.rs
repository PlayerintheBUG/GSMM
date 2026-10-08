use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use anyhow::{Context, Result};

/// Entry nel registro globale di GSMM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerEntry {
    /// Nome identificativo del server (es. "survival", "creative")
    pub name: String,
    /// Percorso assoluto alla cartella del server
    pub path: PathBuf,
}

/// Registro globale di tutti i server gestiti da GSMM.
/// Salvato in ~/.config/gsmm/registry.json
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Registry {
    pub servers: Vec<ServerEntry>,
}

fn registry_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Impossibile trovare la directory di configurazione utente"))?;
    Ok(config_dir.join("gsmm").join("registry.json"))
}

impl Registry {
    /// Carica il registro dal disco. Se non esiste, restituisce un registro vuoto.
    pub fn load() -> Result<Self> {
        let path = registry_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Impossibile leggere il registro da '{}'", path.display()))?;
        let reg: Registry = serde_json::from_str(&content)
            .with_context(|| "Errore nel parsing del registro GSMM")?;
        Ok(reg)
    }

    /// Salva il registro su disco.
    pub fn save(&self) -> Result<()> {
        let path = registry_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)
            .with_context(|| format!("Impossibile salvare il registro in '{}'", path.display()))?;
        Ok(())
    }

    /// Aggiunge o aggiorna un server nel registro.
    pub fn register(&mut self, name: &str, path: &Path) -> Result<()> {
        let abs_path = path.canonicalize()
            .unwrap_or_else(|_| path.to_path_buf());

        // Rimuovi eventuali voci duplicate per nome o percorso
        self.servers.retain(|s| s.name != name && s.path != abs_path);

        self.servers.push(ServerEntry {
            name: name.to_string(),
            path: abs_path,
        });
        self.save()
    }

    /// Rimuove un server dal registro per nome.
    pub fn unregister(&mut self, name: &str) -> Result<bool> {
        let before = self.servers.len();
        self.servers.retain(|s| s.name != name);
        let removed = self.servers.len() < before;
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    /// Cerca un server per nome.
    pub fn find_by_name(&self, name: &str) -> Option<&ServerEntry> {
        self.servers.iter().find(|s| s.name == name)
    }

    /// Cerca un server per percorso (confronta il percorso canonico).
    pub fn find_by_path(&self, path: &Path) -> Option<&ServerEntry> {
        let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.servers.iter().find(|s| s.path == abs)
    }
}
