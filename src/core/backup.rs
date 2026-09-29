use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use anyhow::{Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;
use tar::Builder;

pub fn create_world_backup(server_dir: &Path) -> Result<PathBuf> {
    let backups_dir = server_dir.join("backups");
    std::fs::create_dir_all(&backups_dir)?;

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let backup_filename = format!("world_backup_{}.tar.gz", now);
    let backup_path = backups_dir.join(&backup_filename);

    let tar_gz = File::create(&backup_path)
        .with_context(|| format!("Impossibile creare il file di backup '{}'", backup_path.display()))?;
    let enc = GzEncoder::new(tar_gz, Compression::default());
    let mut tar = Builder::new(enc);

    let mut found_any = false;

    // Standard Minecraft world folders
    for folder_name in &["world", "world_nether", "world_the_end"] {
        let folder_path = server_dir.join(folder_name);
        if folder_path.exists() && folder_path.is_dir() {
            tar.append_dir_all(folder_name, &folder_path)
                .with_context(|| format!("Errore durante l'archiviazione di '{}'", folder_path.display()))?;
            found_any = true;
        }
    }

    // Also include server.properties and .gsmm.json if they exist
    for file_name in &["server.properties", ".gsmm.json"] {
        let file_path = server_dir.join(file_name);
        if file_path.exists() && file_path.is_file() {
            if let Ok(mut f) = File::open(&file_path) {
                let _ = tar.append_file(file_name, &mut f);
            }
        }
    }

    tar.finish()?;

    if !found_any {
        // No world folder yet (e.g. server never started), but backup was created for configs
    }

    Ok(backup_path)
}
