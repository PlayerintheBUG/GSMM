use std::collections::HashSet;
use std::path::Path;
use anyhow::Result;
use colored::*;
use inquire::{Select, Text, Confirm};

use crate::core::config::{LoaderType, ServerConfig};
use crate::core::java::{detect_java, validate_java_compatibility};
use crate::downloader::fabric::FabricClient;
use crate::downloader::mojang::MojangClient;
use crate::downloader::neoforge::NeoForgeClient;
use crate::downloader::papermc::PaperClient;
use crate::downloader::modrinth::ModrinthClient;

pub async fn run_interactive_wizard(server_dir: &Path) -> Result<ServerConfig> {
    println!("{}", "\n═══════════════════════════════════════════════════════════".cyan().bold());
    println!("{}", "        🎮 GSMM - CONFIGURATORE SERVER MINECRAFT          ".green().bold());
    println!("{}", "═══════════════════════════════════════════════════════════\n".cyan().bold());

    // 1. Choose Loader
    let loader_options = vec![
        "Fabric (Consigliato per mod moderne e prestazioni ottime)",
        "Paper (Consigliato per plugin Spigot/Bukkit, server stabili)",
        "Vanilla (Server ufficiale Mojang pulito)",
        "NeoForge (Consigliato per mod complesse 1.20.4+)",
        "Purpur (Fork di Paper ad altissime prestazioni)",
    ];

    let loader_choice = Select::new("Seleziona il tipo di Server / ModLoader:", loader_options).prompt()?;

    let (loader, loader_str) = match loader_choice {
        opt if opt.starts_with("Fabric") => (LoaderType::Fabric, "fabric"),
        opt if opt.starts_with("Paper") => (LoaderType::Paper, "paper"),
        opt if opt.starts_with("Vanilla") => (LoaderType::Vanilla, "vanilla"),
        opt if opt.starts_with("NeoForge") => (LoaderType::NeoForge, "neoforge"),
        opt if opt.starts_with("Purpur") => (LoaderType::Purpur, "purpur"),
        _ => (LoaderType::Fabric, "fabric"),
    };

    // 2. Choose Version
    let mojang_client = MojangClient::new();
    let default_versions = match mojang_client.get_releases().await {
        Ok(releases) => releases.into_iter().take(8).collect::<Vec<_>>(),
        Err(_) => vec![
            "1.21.1".to_string(),
            "1.21".to_string(),
            "1.20.4".to_string(),
            "1.20.1".to_string(),
            "1.19.4".to_string(),
            "1.16.5".to_string(),
        ],
    };

    let mut version_choices = default_versions;
    version_choices.push("Altra versione (scrivi a mano)".to_string());

    let version_select = Select::new("Seleziona la versione di Minecraft:", version_choices).prompt()?;

    let mc_version = if version_select.starts_with("Altra") {
        Text::new("Inserisci la versione di Minecraft (es. 1.20.2):")
            .with_default("1.21.1")
            .prompt()?
    } else {
        version_select
    };

    // 3. RAM configuration
    let ram_options = vec![
        "2048 MB (2 GB - Leggero / Pochi giocatori)",
        "4096 MB (4 GB - Consigliato per Fabric / Paper)",
        "6144 MB (6 GB - Modpack medi)",
        "8192 MB (8 GB - Modpack pesanti)",
        "Personalizzata",
    ];

    let ram_choice = Select::new("Quanta memoria RAM massima allocare al server?", ram_options).prompt()?;

    let ram_max_mb: u32 = if ram_choice.starts_with("Personalizzata") {
        let input = Text::new("Inserisci RAM massima in MB (es. 5120 per 5GB):")
            .with_default("4096")
            .prompt()?;
        input.parse().unwrap_or(4096)
    } else {
        match ram_choice {
            opt if opt.starts_with("2048") => 2048,
            opt if opt.starts_with("4096") => 4096,
            opt if opt.starts_with("6144") => 6144,
            opt if opt.starts_with("8192") => 8192,
            _ => 4096,
        }
    };

    let ram_min_mb = (ram_max_mb / 2).max(1024);

    // 4. Server Port
    let port_str = Text::new("Porta del server:")
        .with_default("25565")
        .prompt()?;
    let port: u16 = port_str.parse().unwrap_or(25565);

    // 5. Server Name / MOTD
    let server_name = Text::new("Nome / Descrizione del Server (MOTD):")
        .with_default("Un fantastico server Minecraft GSMM")
        .prompt()?;

    // 6. Java detection
    println!("\n🔍 Controllo runtime Java nel sistema...");
    if let Ok(java_info) = detect_java(None) {
        validate_java_compatibility(&java_info, &mc_version);
    } else {
        println!("{}", "⚠️ Nessun runtime Java rilevato nel PATH! Assicurati di installare OpenJDK.".yellow());
    }

    // 7. Modrinth Initial Mods (if loader supports mods)
    let is_modded = matches!(loader, LoaderType::Fabric | LoaderType::NeoForge | LoaderType::Forge);
    let mut initial_mods = Vec::new();

    if is_modded {
        let want_mods = Confirm::new("Vuoi aggiungere subito delle mod da Modrinth?")
            .with_default(true)
            .prompt()?;

        if want_mods {
            let suggestions = if loader == LoaderType::Fabric {
                "lithium, ferrite-core, chunky"
            } else {
                "ferrite-core, chunky"
            };

            let mods_input = Text::new("Inserisci nomi/slug di mod da installare (separati da virgola, es. lithium, chunky):")
                .with_default(suggestions)
                .prompt()?;

            for m in mods_input.split(',') {
                let trimmed = m.trim();
                if !trimmed.is_empty() {
                    initial_mods.push(trimmed.to_string());
                }
            }
        }
    }

    println!("\n{}", "🚀 Download e configurazione del server in corso...".cyan().bold());

    // Execute Download
    let jar_name = match loader {
        LoaderType::Vanilla => {
            mojang_client.download_server(&mc_version, server_dir).await?
        }
        LoaderType::Paper | LoaderType::Purpur => {
            let paper_client = PaperClient::new(loader_str);
            paper_client.download_latest_build(&mc_version, server_dir).await?
        }
        LoaderType::Fabric => {
            let fabric_client = FabricClient::new();
            fabric_client.download_server(&mc_version, server_dir).await?
        }
        LoaderType::NeoForge => {
            let neo_client = NeoForgeClient::new();
            neo_client.download_and_install(&mc_version, server_dir, None).await?
        }
        LoaderType::Forge => {
            anyhow::bail!("Per Forge usa NeoForge per versioni moderne (1.20.4+), o Vanilla con Fabric.");
        }
    };

    // Install initial mods if any
    if !initial_mods.is_empty() {
        println!("\n📦 Download mod da Modrinth...");
        let modrinth_client = ModrinthClient::new();
        let mods_dir = server_dir.join("mods");
        let mut installed_set = HashSet::new();

        for mod_slug in &initial_mods {
            match modrinth_client
                .install_mod_with_dependencies(mod_slug, &mc_version, loader_str, &mods_dir, &mut installed_set)
                .await
            {
                Ok(files) => {
                    for f in files {
                        println!("  ✓ Installato: {}", f.green());
                    }
                }
                Err(e) => {
                    println!("  ⚠️ Impossibile installare '{}': {}", mod_slug.yellow(), e);
                }
            }
        }
    }

    // Save configurations
    ServerConfig::ensure_eula(server_dir).await?;
    ServerConfig::ensure_server_properties(server_dir, port, &server_name).await?;

    let config = ServerConfig {
        name: server_name,
        mc_version: mc_version.clone(),
        loader,
        loader_version: None,
        server_jar: jar_name,
        java_path: None,
        ram_min_mb,
        ram_max_mb,
        port,
        jvm_args: ServerConfig::default().jvm_args,
    };

    config.save_to_dir(server_dir).await?;

    println!("\n{}", "═══════════════════════════════════════════════════════════".green().bold());
    println!("{}", "✨ SERVER PRONTO E CONFIGURATO CON SUCCESSO!".green().bold());
    println!("{}", "• Per avviare il server:   gsmm start".white().bold());
    println!("{}", "• Per aprire la Web UI:    gsmm web".white().bold());
    println!("{}", "• Per verificare la rete:  gsmm check".white().bold());
    println!("{}", "• Per gestire le mod:      gsmm mod add <nome>".white().bold());
    println!("{}", "═══════════════════════════════════════════════════════════\n".green().bold());

    Ok(config)
}

pub async fn run_upgrade_wizard(server_dir: &Path) -> Result<ServerConfig> {
    let mut config = ServerConfig::load_from_dir(server_dir).await?;

    println!("{}", "\n═══════════════════════════════════════════════════════════".cyan().bold());
    println!("{}", "        🔄 GSMM - AGGIORNAMENTO / RICONFIGURAZIONE        ".yellow().bold());
    println!("{}", "═══════════════════════════════════════════════════════════".cyan().bold());
    println!("• Configurazione attuale: Minecraft {} ({})", config.mc_version.green(), config.loader.to_string().cyan());
    println!("• Memoria RAM attuale:    {} MB", config.ram_max_mb.to_string().yellow());
    println!("• Directory Mondo:        {} (Verrà preservata intatta!)", "world/".green().bold());
    println!("═══════════════════════════════════════════════════════════\n");

    // 1. Ask for safety backup
    let want_backup = Confirm::new("Vuoi creare una copia di backup del mondo prima di procedere?")
        .with_default(true)
        .prompt()?;

    if want_backup {
        println!("Creazione backup di sicurezza in corso...");
        let backup_path = crate::core::backup::create_world_backup(server_dir)?;
        println!("{}", format!("✓ Backup del mondo salvato con successo in: {}", backup_path.display()).green());
    }

    // 2. Select / input new MC version
    let mojang_client = MojangClient::new();
    let default_versions = match mojang_client.get_releases().await {
        Ok(releases) => releases.into_iter().take(8).collect::<Vec<_>>(),
        Err(_) => vec![
            "1.21.1".to_string(),
            "1.21".to_string(),
            "1.20.4".to_string(),
            "1.20.1".to_string(),
        ],
    };

    let mut version_choices = default_versions;
    let current_opt = format!("Mantieni versione attuale ({})", config.mc_version);
    version_choices.insert(0, current_opt.clone());
    version_choices.push("Altra versione (scrivi a mano)".to_string());

    let version_select = Select::new("Seleziona la nuova versione di Minecraft:", version_choices).prompt()?;

    let new_mc_version = if version_select == current_opt {
        config.mc_version.clone()
    } else if version_select.starts_with("Altra") {
        Text::new("Inserisci la nuova versione di Minecraft:")
            .with_default(&config.mc_version)
            .prompt()?
    } else {
        version_select
    };

    // 3. Keep or change loader
    let loader_choices = vec![
        format!("Mantieni loader attuale ({})", config.loader),
        "Fabric".to_string(),
        "Paper".to_string(),
        "Vanilla".to_string(),
        "NeoForge".to_string(),
        "Purpur".to_string(),
    ];

    let loader_select = Select::new("Modloader del server:", loader_choices).prompt()?;

    let new_loader = if loader_select.starts_with("Mantieni") {
        config.loader.clone()
    } else {
        match loader_select.as_str() {
            "Fabric" => LoaderType::Fabric,
            "Paper" => LoaderType::Paper,
            "Vanilla" => LoaderType::Vanilla,
            "NeoForge" => LoaderType::NeoForge,
            "Purpur" => LoaderType::Purpur,
            _ => config.loader.clone(),
        }
    };

    // 4. Download new server jar
    println!("\n{}", format!("🚀 Scaricamento server per MC {} ({loader_enum})...", new_mc_version, loader_enum = new_loader).cyan().bold());
    let loader_str = format!("{}", new_loader).to_lowercase();
    let jar_name = match new_loader {
        LoaderType::Vanilla => {
            mojang_client.download_server(&new_mc_version, server_dir).await?
        }
        LoaderType::Paper | LoaderType::Purpur => {
            let paper_client = PaperClient::new(&loader_str);
            paper_client.download_latest_build(&new_mc_version, server_dir).await?
        }
        LoaderType::Fabric => {
            let fabric_client = FabricClient::new();
            fabric_client.download_server(&new_mc_version, server_dir).await?
        }
        LoaderType::NeoForge => {
            let neo_client = NeoForgeClient::new();
            neo_client.download_and_install(&new_mc_version, server_dir, None).await?
        }
        LoaderType::Forge => {
            anyhow::bail!("Per Forge usa NeoForge per versioni 1.20.4+");
        }
    };

    // 5. Update configuration
    config.mc_version = new_mc_version.clone();
    config.loader = new_loader;
    config.server_jar = jar_name;
    config.save_to_dir(server_dir).await?;

    // 6. Check existing mods
    let mods_dir = server_dir.join("mods");
    if let Ok(installed_mods) = ModrinthClient::list_installed_mods(&mods_dir).await {
        if !installed_mods.is_empty() {
            println!("{}", "\n⚠️  ATTENZIONE MOD ESISTENTI:".yellow().bold());
            println!("Hai {} mod installate nella cartella mods/.", installed_mods.len());
            println!("Se hai cambiato versione di Minecraft o loader, assicurati che le mod siano compatibili");
            println!("(puoi verificare e riscaricare le versioni aggiornate con '{}').", "gsmm mod add <nome>".cyan());
        }
    }

    println!("\n{}", "═══════════════════════════════════════════════════════════".green().bold());
    println!("{}", "✨ SERVER AGGIORNATO CON SUCCESSO!".green().bold());
    println!("• Nuova versione: {} ({})", config.mc_version.green(), config.loader.to_string().cyan());
    println!("• Il mondo e le impostazioni sono rimasti intatti al 100%.");
    println!("• Per avviare il server aggiornato: {}", "gsmm start".white().bold());
    println!("{}", "═══════════════════════════════════════════════════════════\n".green().bold());

    Ok(config)
}
