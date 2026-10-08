mod core;
mod downloader;
mod network;
mod cli;
mod web;
mod registry;
mod service;

use std::env;
use std::path::PathBuf;
use clap::Parser;
use cli::{handle_command, Cli};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Determine target server directory (either --dir flag or current working directory)
    let server_dir = cli.dir.unwrap_or_else(|| {
        env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    });

    // Create server dir if it doesn't exist
    if !server_dir.exists() {
        tokio::fs::create_dir_all(&server_dir).await?;
    }

    if let Err(e) = handle_command(cli.command, &server_dir).await {
        eprintln!("\n❌ Errore: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
