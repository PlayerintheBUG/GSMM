pub mod mojang;
pub mod papermc;
pub mod fabric;
pub mod neoforge;
pub mod modrinth;

use std::path::Path;
use anyhow::{Context, Result};
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

pub async fn download_file_with_progress(
    client: &Client,
    url: &str,
    target_path: &Path,
    label: &str,
) -> Result<()> {
    let res = client
        .get(url)
        .header("User-Agent", "GSMM/0.1.0 (Minecraft Server Manager)")
        .send()
        .await
        .with_context(|| format!("Errore nella richiesta di download da: {}", url))?;

    if !res.status().is_success() {
        anyhow::bail!("Download fallito con stato HTTP {}: {}", res.status(), url);
    }

    let total_size = res.content_length().unwrap_or(0);

    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")
            .expect("Progress bar template error")
            .progress_chars("#>-"),
    );
    pb.set_message(label.to_string());

    if let Some(parent) = target_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut file = File::create(target_path)
        .await
        .with_context(|| format!("Impossibile creare il file target: {}", target_path.display()))?;

    let mut stream = res.bytes_stream();
    let mut downloaded: u64 = 0;

    while let Some(item) = stream.next().await {
        let chunk = item.context("Errore durante la lettura dello stream di download")?;
        file.write_all(&chunk).await.context("Errore nella scrittura su disco")?;
        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
    }

    file.flush().await?;
    pb.finish_with_message(format!("✓ {} completato", label));

    Ok(())
}
