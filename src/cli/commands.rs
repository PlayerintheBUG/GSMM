use std::collections::HashSet;
use std::path::Path;
use anyhow::Result;
use colored::*;

use super::interactive::run_interactive_wizard;
use super::{Commands, ModCommands, ServerCommands, ServiceCommands};
use crate::core::config::{LoaderType, ServerConfig};
use crate::core::process::ServerManager;
use crate::downloader::fabric::FabricClient;
use crate::downloader::mojang::MojangClient;
use crate::downloader::neoforge::NeoForgeClient;
use crate::downloader::papermc::PaperClient;
use crate::downloader::modrinth::ModrinthClient;
use crate::network::port_check::{generate_diagnostics_report, print_diagnostics_report};
use crate::registry::Registry;
use crate::web::server::start_web_server;

pub async fn handle_command(cmd_opt: Option<Commands>, server_dir: &Path) -> Result<()> {
    match cmd_opt {
        None => {
            let config_res = ServerConfig::load_from_dir(server_dir).await;
            if config_res.is_ok() {
                println!("Server già configurato in questa directory: {}", server_dir.display());
                println!("Digita '{}' per avviare il server, o '{}' per l'interfaccia web.", "gsmm start".green(), "gsmm web".cyan());
                println!("Digita '{}' per riconfigurare o '{}' per la guida.", "gsmm init".yellow(), "gsmm --help".white());
            } else {
                run_interactive_wizard(server_dir).await?;
            }
        }
        Some(Commands::Init { version, loader, ram, mods, name }) => {
            if version.is_none() && loader.is_none() {
                run_interactive_wizard(server_dir).await?;
            } else {
                let mc_version = version.unwrap_or_else(|| "1.21.1".to_string());
                let loader_enum = loader
                    .as_deref()
                    .unwrap_or("fabric")
                    .parse::<LoaderType>()
                    .unwrap_or(LoaderType::Fabric);

                let ram_max_mb = ram.unwrap_or(4096);
                let ram_min_mb = (ram_max_mb / 2).max(1024);

                let server_name = name.unwrap_or_else(|| {
                    server_dir.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("server")
                        .to_string()
                });

                println!("Configurazione server automatica (Versione: {}, Loader: {})...", mc_version, loader_enum);

                let loader_str = loader_enum.to_string().to_lowercase();
                let jar_name = match loader_enum {
                    LoaderType::Vanilla => {
                        let client = MojangClient::new();
                        client.download_server(&mc_version, server_dir).await?
                    }
                    LoaderType::Paper | LoaderType::Purpur => {
                        let client = PaperClient::new(&loader_str);
                        client.download_latest_build(&mc_version, server_dir).await?
                    }
                    LoaderType::Fabric => {
                        let client = FabricClient::new();
                        client.download_server(&mc_version, server_dir).await?
                    }
                    LoaderType::NeoForge => {
                        let client = NeoForgeClient::new();
                        client.download_and_install(&mc_version, server_dir, None).await?
                    }
                    LoaderType::Forge => {
                        anyhow::bail!("Per Forge usa NeoForge per versioni 1.20.4+");
                    }
                };

                if let Some(mod_list) = mods {
                    let modrinth = ModrinthClient::new();
                    let mods_dir = server_dir.join("mods");
                    let mut installed = HashSet::new();
                    for m in mod_list.split(',') {
                        let trimmed = m.trim();
                        if !trimmed.is_empty() {
                            let _ = modrinth.install_mod_with_dependencies(trimmed, &mc_version, &loader_str, &mods_dir, &mut installed).await;
                        }
                    }
                }

                ServerConfig::ensure_eula(server_dir).await?;
                ServerConfig::ensure_server_properties(server_dir, 25565, "Minecraft Server GSMM").await?;

                let config = ServerConfig {
                    name: server_name.clone(),
                    mc_version,
                    loader: loader_enum,
                    loader_version: None,
                    server_jar: jar_name,
                    java_path: None,
                    ram_min_mb,
                    ram_max_mb,
                    port: 25565,
                    jvm_args: ServerConfig::default().jvm_args,
                };
                config.save_to_dir(server_dir).await?;

                // Auto-registra nel registro globale
                if let Ok(mut reg) = Registry::load() {
                    let _ = reg.register(&server_name, server_dir);
                    println!("{}", format!("✓ Server '{}' registrato nel registro globale.", server_name).cyan());
                }

                println!("{}", "✓ Server inizializzato e configurato con successo!".green().bold());
            }
        }
        Some(Commands::Start { ram }) => {
            let mut config = ServerConfig::load_from_dir(server_dir).await?;
            if let Some(r) = ram {
                config.ram_max_mb = r;
                config.ram_min_mb = (r / 2).max(1024);
            }
            let manager = ServerManager::new();
            manager.run_attached_cli(server_dir, &config).await?;
        }
        Some(Commands::Stop) => {
            println!("Per fermare il server in console, digita 'stop' o invia SIGINT (Ctrl+C).");
        }
        Some(Commands::Status) => {
            show_status_cli(server_dir).await?;
        }
        Some(Commands::Upgrade { version, loader, backup }) => {
            if version.is_none() && loader.is_none() {
                super::interactive::run_upgrade_wizard(server_dir).await?;
            } else {
                let current_cfg = ServerConfig::load_from_dir(server_dir).await?;
                let target_version = version.unwrap_or(current_cfg.mc_version);
                let target_loader = if let Some(l) = loader {
                    Some(l.parse::<LoaderType>()?)
                } else {
                    None
                };
                let do_backup = backup.unwrap_or(true);
                let res_msg = upgrade_server_core(server_dir, &target_version, target_loader, do_backup).await?;
                println!("{}", format!("✓ {}", res_msg).green().bold());
            }
        }
        Some(Commands::Backup) => {
            println!("Creazione copia di backup del mondo e configurazioni...");
            let bpath = crate::core::backup::create_world_backup(server_dir)?;
            println!("{}", format!("✓ Backup completato in: {}", bpath.display()).green().bold());
        }
        Some(Commands::Mod(mod_cmd)) => match mod_cmd {
            ModCommands::Search { query, limit } => {
                search_mods_cli(&query, limit, server_dir).await?;
            }
            ModCommands::Add { slugs } => {
                add_mods_cli(&slugs, server_dir).await?;
            }
            ModCommands::List => {
                list_mods_cli(server_dir).await?;
            }
            ModCommands::Remove { filename } => {
                remove_mod_cli(&filename, server_dir).await?;
            }
        },
        Some(Commands::Check { port }) => {
            check_network_cli(port, server_dir).await?;
        }
        Some(Commands::Web { port, no_open }) => {
            // Passa la dir corrente come server default solo se ha una config
            let maybe_dir = if ServerConfig::load_from_dir(server_dir).await.is_ok() {
                Some(server_dir)
            } else {
                None
            };
            start_web_server(maybe_dir, port, !no_open).await?;
        }
        Some(Commands::Server(server_cmd)) => {
            handle_server_command(server_cmd, server_dir).await?;
        }
        Some(Commands::Service(service_cmd)) => {
            handle_service_command(service_cmd, server_dir).await?;
        }
    }

    Ok(())
}

// ─── Server registry commands ─────────────────────────────────────────────────

async fn handle_server_command(cmd: ServerCommands, server_dir: &Path) -> Result<()> {
    match cmd {
        ServerCommands::List => {
            let reg = Registry::load()?;
            if reg.servers.is_empty() {
                println!("{}", "Nessun server registrato. Usa 'gsmm server add <nome>' o 'gsmm init'.".yellow());
            } else {
                println!("{}", "═══════════════════════════════════════════════════════════".cyan());
                println!("{}", "   📋 SERVER REGISTRATI IN GSMM".green().bold());
                println!("{}", "═══════════════════════════════════════════════════════════".cyan());
                for entry in &reg.servers {
                    let exists = entry.path.exists();
                    let cfg = ServerConfig::load_from_dir(&entry.path).await.ok();
                    let version_info = cfg.as_ref()
                        .map(|c| format!(" | MC {} ({})", c.mc_version, c.loader))
                        .unwrap_or_default();
                    let status_icon = if exists { "✓".green() } else { "✗ (percorso non trovato)".red() };
                    println!("  {} {} — {}{}", status_icon, entry.name.cyan().bold(), entry.path.display(), version_info.yellow());
                }
                println!("{}", "═══════════════════════════════════════════════════════════".cyan());
                println!("  Totale: {} server", reg.servers.len());
            }
        }
        ServerCommands::Add { name, path } => {
            let target = path.unwrap_or_else(|| server_dir.to_path_buf());
            if !target.exists() {
                anyhow::bail!("Il percorso '{}' non esiste.", target.display());
            }
            let mut reg = Registry::load()?;
            reg.register(&name, &target)?;
            println!("{}", format!("✓ Server '{}' aggiunto al registro ({}).", name, target.display()).green().bold());
        }
        ServerCommands::Remove { name } => {
            let mut reg = Registry::load()?;
            if reg.unregister(&name)? {
                println!("{}", format!("✓ Server '{}' rimosso dal registro (i file non sono stati eliminati).", name).green().bold());
            } else {
                anyhow::bail!("Server '{}' non trovato nel registro.", name);
            }
        }
    }
    Ok(())
}

// ─── Service commands ─────────────────────────────────────────────────────────

async fn handle_service_command(cmd: ServiceCommands, server_dir: &Path) -> Result<()> {
    match cmd {
        ServiceCommands::Install { name, bin } => {
            let service_name = if let Some(n) = name {
                n
            } else {
                ServerConfig::load_from_dir(server_dir).await
                    .map(|c| c.name.replace(' ', "-").to_lowercase())
                    .unwrap_or_else(|_| {
                        server_dir.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("minecraft")
                            .to_string()
                    })
            };
            println!("Installazione servizio di sistema per server '{}'...", service_name.cyan());
            crate::service::install_service(server_dir, &service_name, bin.as_deref()).await?;
        }
        ServiceCommands::Uninstall { name } => {
            println!("Rimozione servizio di sistema '{}'...", name.cyan());
            crate::service::uninstall_service(&name).await?;
        }
        ServiceCommands::Status { name } => {
            crate::service::service_status(&name).await?;
        }
    }
    Ok(())
}

// ─── Status / Search / Add / List / Remove ────────────────────────────────────

pub async fn show_status_cli(server_dir: &Path) -> Result<()> {
    let config = ServerConfig::load_from_dir(server_dir).await?;
    println!("{}", "═══════════════════════════════════════════════════════════".cyan());
    println!("  📋 STATO CONFIGURAZIONE SERVER GSMM");
    println!("{}", "═══════════════════════════════════════════════════════════".cyan());
    println!("• Nome:          {}", config.name.green());
    println!("• Versione MC:   {}", config.mc_version.yellow());
    println!("• ModLoader:     {}", config.loader.to_string().cyan());
    println!("• File Server:   {}", config.server_jar);
    println!("• Porta:         {}", config.port);
    println!("• Memoria RAM:   {} MB min / {} MB max", config.ram_min_mb, config.ram_max_mb);

    let mods_dir = server_dir.join("mods");
    if let Ok(installed_mods) = ModrinthClient::list_installed_mods(&mods_dir).await {
        println!("• Mod installate ({}):", installed_mods.len());
        for m in installed_mods {
            println!("  - {}", m);
        }
    }
    println!("{}", "═══════════════════════════════════════════════════════════".cyan());
    Ok(())
}

pub async fn search_mods_cli(query: &str, limit: u32, server_dir: &Path) -> Result<()> {
    let config = ServerConfig::load_from_dir(server_dir).await?;
    let loader_str = config.loader.to_string().to_lowercase();
    let modrinth = ModrinthClient::new();
    println!("Ricerca mod '{}' per MC {} (loader: {})...", query, config.mc_version, loader_str);
    let results = modrinth.search_mods(query, Some(&config.mc_version), Some(&loader_str), limit).await?;
    if results.is_empty() {
        println!("Nessuna mod trovata per '{}'.", query);
    } else {
        println!("\nRisultati trovati:");
        for hit in results {
            println!("• {} ({})", hit.title.green().bold(), hit.slug.cyan());
            println!("  ↳ {}", hit.description);
            println!("  ↳ Download totali: {}", hit.downloads);
        }
    }
    Ok(())
}

pub async fn add_mods_cli(slugs: &[String], server_dir: &Path) -> Result<()> {
    if slugs.is_empty() {
        anyhow::bail!("Specifica almeno uno slug o nome di mod da installare.");
    }
    let config = ServerConfig::load_from_dir(server_dir).await?;
    let loader_str = config.loader.to_string().to_lowercase();
    let modrinth = ModrinthClient::new();
    let mods_dir = server_dir.join("mods");
    let mut installed = HashSet::new();
    for slug in slugs {
        println!("\nDownload di '{}' da Modrinth...", slug);
        match modrinth.install_mod_with_dependencies(slug, &config.mc_version, &loader_str, &mods_dir, &mut installed).await {
            Ok(files) => {
                for f in files {
                    println!("✓ Installato: {}", f.green());
                }
            }
            Err(e) => eprintln!("✗ Errore nell'installazione di '{}': {}", slug, e),
        }
    }
    Ok(())
}

pub async fn list_mods_cli(server_dir: &Path) -> Result<()> {
    let mods_dir = server_dir.join("mods");
    let mods = ModrinthClient::list_installed_mods(&mods_dir).await?;
    println!("\n📦 Mod installate in '{}' (totale: {}):", mods_dir.display(), mods.len());
    for m in mods {
        println!("• {}", m.green());
    }
    Ok(())
}

pub async fn remove_mod_cli(filename: &str, server_dir: &Path) -> Result<()> {
    let mods_dir = server_dir.join("mods");
    let file_path = mods_dir.join(filename);
    if file_path.exists() {
        tokio::fs::remove_file(&file_path).await?;
        println!("✓ Rimossa mod: {}", filename.green());
    } else {
        let with_jar = mods_dir.join(format!("{}.jar", filename));
        if with_jar.exists() {
            tokio::fs::remove_file(&with_jar).await?;
            println!("✓ Rimossa mod: {}.jar", filename.green());
        } else {
            anyhow::bail!("File mod '{}' non trovato in {}", filename, mods_dir.display());
        }
    }
    Ok(())
}

pub async fn check_network_cli(port: Option<u16>, server_dir: &Path) -> Result<()> {
    let config_port = ServerConfig::load_from_dir(server_dir)
        .await
        .map(|c| c.port)
        .unwrap_or(25565);

    let check_port = port.unwrap_or(config_port);
    println!("Verifica connettività di rete per la porta {}...", check_port);
    let report = generate_diagnostics_report(check_port).await;
    print_diagnostics_report(&report);
    Ok(())
}

pub async fn upgrade_server_core(
    server_dir: &Path,
    new_version: &str,
    new_loader: Option<LoaderType>,
    do_backup: bool,
) -> Result<String> {
    if do_backup {
        println!("Creazione copia di backup del mondo...");
        let bpath = crate::core::backup::create_world_backup(server_dir)?;
        println!("{}", format!("✓ Backup del mondo salvato in: {}", bpath.display()).green());
    }

    let mut config = ServerConfig::load_from_dir(server_dir).await?;
    let loader = new_loader.unwrap_or(config.loader);
    let loader_str = loader.to_string().to_lowercase();

    println!("Scaricamento server per MC {} ({loader})...", new_version);
    let jar_name = match loader {
        LoaderType::Vanilla => {
            let client = MojangClient::new();
            client.download_server(new_version, server_dir).await?
        }
        LoaderType::Paper | LoaderType::Purpur => {
            let client = PaperClient::new(&loader_str);
            client.download_latest_build(new_version, server_dir).await?
        }
        LoaderType::Fabric => {
            let client = FabricClient::new();
            client.download_server(new_version, server_dir).await?
        }
        LoaderType::NeoForge => {
            let client = NeoForgeClient::new();
            client.download_and_install(new_version, server_dir, None).await?
        }
        LoaderType::Forge => {
            anyhow::bail!("Per Forge usa NeoForge per versioni 1.20.4+");
        }
    };

    config.mc_version = new_version.to_string();
    config.loader = loader;
    config.server_jar = jar_name;
    config.save_to_dir(server_dir).await?;

    Ok(format!("Server aggiornato con successo a Minecraft {} ({}) preservando il mondo!", config.mc_version, config.loader))
}

pub async fn handle_console_input(input: &str, server_dir: &Path) -> Result<bool> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(false);
    }

    let gsmm_args: Option<Vec<&str>> = if let Some(stripped) = trimmed.strip_prefix("gsmm ") {
        Some(stripped.split_whitespace().collect())
    } else if let Some(stripped) = trimmed.strip_prefix('!') {
        Some(stripped.split_whitespace().collect())
    } else if trimmed == "gsmm" || trimmed == "!help" || trimmed == "gsmm help" {
        Some(vec!["help"])
    } else {
        None
    };

    if let Some(args) = gsmm_args {
        if args.is_empty() || args[0] == "help" {
            println!("{}", "═══════════════════════════════════════════════════════════".cyan());
            println!("{}", "   🛠️  COMANDI GSMM DISPONIBILI NELLA CONSOLE LIVE        ".green().bold());
            println!("{}", "═══════════════════════════════════════════════════════════".cyan());
            println!("• {} o {}       - Mostra IP pubblico, LAN e stato porta", "!check".yellow(), "gsmm check".yellow());
            println!("• {} o {}      - Mostra lo stato e le mod del server", "!status".yellow(), "gsmm status".yellow());
            println!("• {} o {}      - Crea un backup istantaneo del mondo", "!backup".yellow(), "gsmm backup".yellow());
            println!("• {} <nome>     - Cerca mod su Modrinth", "!mod search".yellow());
            println!("• {} <mod1> <mod2> - Installa mod da Modrinth nella cartella mods", "!mod add".yellow());
            println!("• {}           - Elenca le mod installate", "!mod list".yellow());
            println!("• {} <file>        - Rimuove una mod", "!mod remove".yellow());
            println!("• {}                - Arresta il server Minecraft", "stop".red());
            println!("{}", "═══════════════════════════════════════════════════════════".cyan());
            println!("{}", "(Tutti gli altri comandi come 'op', 'say', 'gamemode' vengono inviati a Minecraft)\n".white());
            return Ok(true);
        }

        match args[0] {
            "check" | "ip" => {
                let _ = check_network_cli(None, server_dir).await;
                return Ok(true);
            }
            "status" => {
                let _ = show_status_cli(server_dir).await;
                return Ok(true);
            }
            "backup" => {
                println!("Creazione backup del mondo in corso...");
                match crate::core::backup::create_world_backup(server_dir) {
                    Ok(p) => println!("{}", format!("✓ Backup creato in: {}", p.display()).green().bold()),
                    Err(e) => eprintln!("✗ Errore nella creazione del backup: {}", e),
                }
                return Ok(true);
            }
            "mod" => {
                if args.len() < 2 {
                    println!("Usa: !mod search <nome>, !mod add <slug>, !mod list, !mod remove <file>");
                    return Ok(true);
                }
                match args[1] {
                    "search" => {
                        let q = args[2..].join(" ");
                        let _ = search_mods_cli(&q, 5, server_dir).await;
                    }
                    "add" => {
                        let slugs: Vec<String> = args[2..].iter().map(|s| s.to_string()).collect();
                        let _ = add_mods_cli(&slugs, server_dir).await;
                        println!("{}", "💡 Nota: Le mod sono state scaricate in mods/. Verranno caricate al prossimo riavvio del server.".yellow());
                    }
                    "list" => {
                        let _ = list_mods_cli(server_dir).await;
                    }
                    "remove" | "rm" => {
                        if let Some(file) = args.get(2) {
                            let _ = remove_mod_cli(file, server_dir).await;
                        } else {
                            println!("Specifica il nome del file da rimuovere.");
                        }
                    }
                    other => {
                        println!("Sottocomando mod sconosciuto: '{}'. Usa: search, add, list, remove", other);
                    }
                }
                return Ok(true);
            }
            other => {
                println!("Comando GSMM '{}' non riconosciuto. Digita '!help' per la lista dei comandi.", other);
                return Ok(true);
            }
        }
    }

    Ok(false)
}
