use std::path::Path;
use anyhow::Result;
use colored::*;

/// Installa GSMM come servizio systemd (Linux) o Task Scheduler (Windows).
///
/// Parametri:
/// - `server_dir`: cartella del server Minecraft
/// - `server_name`: nome identificativo del servizio
/// - `binary_path`: percorso assoluto al binario gsmm (se None, usa `which gsmm` o il binario corrente)
pub async fn install_service(server_dir: &Path, server_name: &str, binary_path: Option<&str>) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        install_systemd(server_dir, server_name, binary_path).await
    }

    #[cfg(target_os = "macos")]
    {
        install_launchd(server_dir, server_name, binary_path).await
    }

    #[cfg(target_os = "windows")]
    {
        install_windows_task(server_dir, server_name, binary_path).await
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        anyhow::bail!("Installazione servizio non supportata su questo sistema operativo")
    }
}

/// Rimuove il servizio di sistema per il server specificato.
pub async fn uninstall_service(server_name: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        uninstall_systemd(server_name).await
    }

    #[cfg(target_os = "macos")]
    {
        uninstall_launchd(server_name).await
    }

    #[cfg(target_os = "windows")]
    {
        uninstall_windows_task(server_name).await
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        anyhow::bail!("Rimozione servizio non supportata su questo sistema operativo")
    }
}

/// Mostra lo stato del servizio di sistema.
pub async fn service_status(server_name: &str) -> Result<()> {
    let unit = format!("gsmm-{}.service", server_name);

    #[cfg(target_os = "linux")]
    {
        let out = tokio::process::Command::new("systemctl")
            .args(["status", &unit, "--no-pager"])
            .output()
            .await;
        match out {
            Ok(o) => {
                let text = String::from_utf8_lossy(&o.stdout);
                println!("{}", text);
            }
            Err(e) => anyhow::bail!("Impossibile interrogare systemctl: {}", e),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        println!("Stato del servizio '{}': controlla il gestore servizi del tuo sistema.", server_name);
    }

    Ok(())
}

// ─── Linux / systemd ─────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
async fn install_systemd(server_dir: &Path, server_name: &str, binary_path: Option<&str>) -> Result<()> {
    let unit_name = format!("gsmm-{}", server_name);
    let service_file = format!("/etc/systemd/system/{}.service", unit_name);

    // Percorso del binario: usa quello specificato, altrimenti cerca in PATH
    let bin = if let Some(b) = binary_path {
        b.to_string()
    } else {
        which_gsmm()
    };

    let abs_dir = server_dir.canonicalize()
        .unwrap_or_else(|_| server_dir.to_path_buf());

    // Rileva l'utente corrente per il campo User=
    let current_user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "root".to_string());

    let unit_content = format!(
        "[Unit]\n\
         Description=GSMM Minecraft Server — {name}\n\
         Documentation=https://github.com/PlayerintheBUG/GSMM\n\
         After=network.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         User={user}\n\
         WorkingDirectory={dir}\n\
         ExecStart={bin} start\n\
         Restart=always\n\
         RestartSec=10\n\
         StandardOutput=journal\n\
         StandardError=journal\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n",
        name = server_name,
        user = current_user,
        dir = abs_dir.display(),
        bin = bin,
    );

    // Prova a scrivere direttamente il file di sistema
    // Se non si hanno i permessi, mostra le istruzioni per farlo manualmente
    match std::fs::write(&service_file, &unit_content) {
        Ok(_) => {
            println!("{}", format!("✓ File servizio scritto in: {}", service_file).green().bold());
            // Ricarica systemd e abilita il servizio
            let _ = tokio::process::Command::new("systemctl")
                .args(["daemon-reload"])
                .status().await;
            let _ = tokio::process::Command::new("systemctl")
                .args(["enable", &format!("{}.service", unit_name)])
                .status().await;
            println!("{}", format!("✓ Servizio '{}' abilitato all'avvio!", unit_name).green().bold());
            println!("  Avvia subito con: {}", format!("sudo systemctl start {}", unit_name).yellow());
        }
        Err(_) => {
            // Non si hanno i permessi root: mostra come farlo manualmente
            println!("{}", "⚠️  Permessi insufficienti per scrivere in /etc/systemd/system/".yellow().bold());
            println!("{}", "   Esegui i seguenti comandi come root per installare il servizio:\n".white());

            // Scrivi il file in /tmp e mostra le istruzioni
            let tmp_file = format!("/tmp/{}.service", unit_name);
            std::fs::write(&tmp_file, &unit_content)?;
            println!("  {}", format!("sudo cp {} {}", tmp_file, service_file).cyan());
            println!("  {}", "sudo systemctl daemon-reload".cyan());
            println!("  {}", format!("sudo systemctl enable {}.service", unit_name).cyan());
            println!("  {}", format!("sudo systemctl start {}.service", unit_name).cyan());
            println!();
            println!("{}", format!("  Il file servizio è stato salvato in: {}", tmp_file).white());
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
async fn uninstall_systemd(server_name: &str) -> Result<()> {
    let unit_name = format!("gsmm-{}", server_name);
    let service_file = format!("/etc/systemd/system/{}.service", unit_name);

    let _ = tokio::process::Command::new("systemctl")
        .args(["stop", &format!("{}.service", unit_name)])
        .status().await;
    let _ = tokio::process::Command::new("systemctl")
        .args(["disable", &format!("{}.service", unit_name)])
        .status().await;

    match std::fs::remove_file(&service_file) {
        Ok(_) => {
            let _ = tokio::process::Command::new("systemctl")
                .args(["daemon-reload"])
                .status().await;
            println!("{}", format!("✓ Servizio '{}' rimosso con successo.", unit_name).green().bold());
        }
        Err(_) => {
            println!("{}", "⚠️  Permessi insufficienti. Esegui manualmente:".yellow());
            println!("  {}", format!("sudo systemctl stop {}.service", unit_name).cyan());
            println!("  {}", format!("sudo systemctl disable {}.service", unit_name).cyan());
            println!("  {}", format!("sudo rm {}", service_file).cyan());
            println!("  {}", "sudo systemctl daemon-reload".cyan());
        }
    }

    Ok(())
}

// ─── macOS / launchd ──────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
async fn install_launchd(server_dir: &Path, server_name: &str, binary_path: Option<&str>) -> Result<()> {
    let label = format!("com.gsmm.{}", server_name);
    let plist_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Home directory non trovata"))?
        .join("Library/LaunchAgents");
    let plist_file = plist_dir.join(format!("{}.plist", label));

    let bin = if let Some(b) = binary_path { b.to_string() } else { which_gsmm() };
    let abs_dir = server_dir.canonicalize().unwrap_or_else(|_| server_dir.to_path_buf());

    let plist_content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
           <key>Label</key><string>{label}</string>\n\
           <key>ProgramArguments</key>\n\
           <array><string>{bin}</string><string>start</string></array>\n\
           <key>WorkingDirectory</key><string>{dir}</string>\n\
           <key>RunAtLoad</key><true/>\n\
           <key>KeepAlive</key><true/>\n\
           <key>StandardOutPath</key><string>{dir}/gsmm.log</string>\n\
           <key>StandardErrorPath</key><string>{dir}/gsmm-error.log</string>\n\
         </dict>\n\
         </plist>\n",
        label = label,
        bin = bin,
        dir = abs_dir.display(),
    );

    std::fs::create_dir_all(&plist_dir)?;
    std::fs::write(&plist_file, plist_content)?;

    let _ = tokio::process::Command::new("launchctl")
        .args(["load", plist_file.to_str().unwrap_or("")])
        .status().await;

    println!("{}", format!("✓ Servizio LaunchAgent '{}' installato!", label).green().bold());
    println!("  Il server si avvierà automaticamente al login.");
    Ok(())
}

#[cfg(target_os = "macos")]
async fn uninstall_launchd(server_name: &str) -> Result<()> {
    let label = format!("com.gsmm.{}", server_name);
    let plist_file = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Home directory non trovata"))?
        .join(format!("Library/LaunchAgents/{}.plist", label));

    let _ = tokio::process::Command::new("launchctl")
        .args(["unload", plist_file.to_str().unwrap_or("")])
        .status().await;

    if plist_file.exists() {
        std::fs::remove_file(&plist_file)?;
    }

    println!("{}", format!("✓ Servizio '{}' rimosso.", label).green().bold());
    Ok(())
}

// ─── Windows / Task Scheduler ────────────────────────────────────────────────

#[cfg(target_os = "windows")]
async fn install_windows_task(server_dir: &Path, server_name: &str, binary_path: Option<&str>) -> Result<()> {
    let task_name = format!("GSMM_{}", server_name);
    let bin = if let Some(b) = binary_path { b.to_string() } else { which_gsmm() };
    let abs_dir = server_dir.canonicalize().unwrap_or_else(|_| server_dir.to_path_buf());

    // schtasks /Create per avvio automatico
    let out = tokio::process::Command::new("schtasks")
        .args([
            "/Create",
            "/TN", &task_name,
            "/TR", &format!("\"{}\" start", bin),
            "/SC", "ONSTART",
            "/RU", "SYSTEM",
            "/RL", "HIGHEST",
            "/F", // Forza sovrascrittura se esiste
        ])
        .output()
        .await?;

    if out.status.success() {
        println!("{}", format!("✓ Task '{}' creato in Task Scheduler Windows!", task_name).green().bold());
        println!("  Il server si avvierà automaticamente ad ogni avvio di Windows.");
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("Errore nella creazione del task: {}", err);
    }

    Ok(())
}

#[cfg(target_os = "windows")]
async fn uninstall_windows_task(server_name: &str) -> Result<()> {
    let task_name = format!("GSMM_{}", server_name);
    let out = tokio::process::Command::new("schtasks")
        .args(["/Delete", "/TN", &task_name, "/F"])
        .output()
        .await?;

    if out.status.success() {
        println!("{}", format!("✓ Task '{}' rimosso da Task Scheduler.", task_name).green().bold());
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("Errore nella rimozione del task: {}", err);
    }
    Ok(())
}

// ─── Utility ─────────────────────────────────────────────────────────────────

/// Trova il percorso del binario gsmm attualmente in esecuzione o nel PATH.
fn which_gsmm() -> String {
    // Prima prova con il processo corrente (percorso esatto del binario in esecuzione)
    if let Ok(path) = std::env::current_exe() {
        return path.to_string_lossy().to_string();
    }
    // Fallback: cerca in PATH
    "gsmm".to_string()
}
