use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{broadcast, Mutex};
use anyhow::{Context, Result};
use colored::*;

use super::config::ServerConfig;
use super::java::detect_java;

pub struct RunningServer {
    pub child: Child,
    pub stdin: Option<ChildStdin>,
}

pub struct ServerManager {
    inner: Arc<Mutex<Option<RunningServer>>>,
    log_sender: broadcast::Sender<String>,
}

impl ServerManager {
    pub fn new() -> Self {
        let (log_sender, _) = broadcast::channel(500);
        Self {
            inner: Arc::new(Mutex::new(None)),
            log_sender,
        }
    }

    pub fn subscribe_logs(&self) -> broadcast::Receiver<String> {
        self.log_sender.subscribe()
    }

    pub async fn is_running(&self) -> bool {
        let mut lock = self.inner.lock().await;
        if let Some(server) = lock.as_mut() {
            match server.child.try_wait() {
                Ok(Some(_status)) => {
                    *lock = None;
                    false
                }
                Ok(None) => true,
                Err(_) => {
                    *lock = None;
                    false
                }
            }
        } else {
            false
        }
    }

    pub async fn start(&self, dir: &Path, config: &ServerConfig) -> Result<()> {
        if self.is_running().await {
            anyhow::bail!("Il server è già in esecuzione!");
        }

        let java = detect_java(config.java_path.as_deref())?;
        let jar_path = dir.join(&config.server_jar);
        if !jar_path.exists() {
            anyhow::bail!("Il file jar del server '{}' non esiste nella directory!", jar_path.display());
        }

        // Ensure eula is accepted
        ServerConfig::ensure_eula(dir).await?;

        let mut cmd = Command::new(&java.path);
        cmd.current_dir(dir);

        // Memory arguments
        cmd.arg(format!("-Xms{}M", config.ram_min_mb));
        cmd.arg(format!("-Xmx{}M", config.ram_max_mb));

        // JVM args
        for arg in &config.jvm_args {
            cmd.arg(arg);
        }

        // Jar argument
        cmd.arg("-jar");
        cmd.arg(&config.server_jar);
        cmd.arg("nogui");

        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().with_context(|| format!("Impossibile avviare il processo Java con '{}'", java.path.display()))?;

        let stdout = child.stdout.take().context("Impossibile catturare stdout")?;
        let stderr = child.stderr.take().context("Impossibile catturare stderr")?;
        let stdin = child.stdin.take();

        let sender_out = self.log_sender.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = sender_out.send(line);
            }
        });

        let sender_err = self.log_sender.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = sender_err.send(format!("[STDERR] {}", line));
            }
        });

        let mut lock = self.inner.lock().await;
        *lock = Some(RunningServer { child, stdin });

        Ok(())
    }

    pub async fn send_command(&self, cmd_text: &str) -> Result<()> {
        let mut lock = self.inner.lock().await;
        if let Some(server) = lock.as_mut() {
            if let Some(stdin) = server.stdin.as_mut() {
                let formatted = format!("{}\n", cmd_text.trim());
                stdin.write_all(formatted.as_bytes()).await
                    .context("Impossibile inviare il comando allo stdin del server")?;
                stdin.flush().await.context("Flush stdin fallito")?;
                let _ = self.log_sender.send(format!("> {}", cmd_text.trim()));
                return Ok(());
            }
        }
        anyhow::bail!("Il server non è in esecuzione!");
    }

    pub async fn stop(&self) -> Result<()> {
        if !self.is_running().await {
            anyhow::bail!("Il server non è in esecuzione!");
        }

        // Send graceful 'stop' command
        let _ = self.send_command("stop").await;

        // Wait up to 15 seconds for clean exit
        let inner = self.inner.clone();
        tokio::spawn(async move {
            for _ in 0..30 {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                let mut lock = inner.lock().await;
                if let Some(server) = lock.as_mut() {
                    if let Ok(Some(_)) = server.child.try_wait() {
                        *lock = None;
                        return;
                    }
                } else {
                    return;
                }
            }

            // If still running after 15s, kill forcefully
            let mut lock = inner.lock().await;
            if let Some(server) = lock.as_mut() {
                let _ = server.child.kill().await;
                *lock = None;
            }
        });

        Ok(())
    }

    pub async fn run_attached_cli(&self, dir: &Path, config: &ServerConfig) -> Result<()> {
        self.start(dir, config).await?;
        println!("{}", "═══════════════════════════════════════════════════════════".cyan());
        println!("{}", format!("✓ Server '{}' avviato con successo!", config.name).green().bold());
        println!("{}", "• Comandi Minecraft:  Digita direttamente 'list', 'op <nome>', 'stop' ecc.".white());
        println!("{}", "• Comandi GSMM:       Usa '!check', '!mod add <nome>', '!status', '!help' o 'gsmm ...'.".cyan());
        println!("{}", "• Arresto:            Scrivi 'stop' o premi Ctrl+C.".yellow());
        println!("{}", "═══════════════════════════════════════════════════════════\n".cyan());

        let mut log_rx = self.subscribe_logs();
        let manager_cmd = self.clone_handle();

        // Task to print logs to terminal
        tokio::spawn(async move {
            while let Ok(line) = log_rx.recv().await {
                if line.starts_with("> ") {
                    println!("{}", line.yellow());
                } else if line.contains("WARN") {
                    println!("{}", line.yellow());
                } else if line.contains("ERROR") || line.contains("[STDERR]") {
                    println!("{}", line.red());
                } else {
                    println!("{}", line);
                }
            }
        });

        // Loop reading stdin from user
        let mut stdin_reader = BufReader::new(tokio::io::stdin()).lines();
        while let Ok(Some(line)) = stdin_reader.next_line().await {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if !manager_cmd.is_running().await {
                println!("{}", "Il server si è arrestato.".red());
                break;
            }

            // Check if it's a GSMM command (!check, !mod add, gsmm check, etc.)
            match crate::cli::handle_console_input(trimmed, dir).await {
                Ok(true) => {
                    // Handled locally by GSMM
                    continue;
                }
                Err(e) => {
                    eprintln!("Errore comando GSMM: {}", e);
                    continue;
                }
                Ok(false) => {
                    // Pass to Minecraft process stdin
                }
            }

            if let Err(e) = manager_cmd.send_command(trimmed).await {
                eprintln!("Errore nell'invio del comando: {}", e);
            }

            if trimmed == "stop" {
                println!("{}", "Arresto del server in corso...".yellow());
                // Wait for process to end
                while manager_cmd.is_running().await {
                    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
                }
                println!("{}", "✓ Server arrestato correttamente.".green());
                break;
            }
        }

        Ok(())
    }

    pub fn clone_handle(&self) -> Arc<Self> {
        Arc::new(Self {
            inner: self.inner.clone(),
            log_sender: self.log_sender.clone(),
        })
    }
}
