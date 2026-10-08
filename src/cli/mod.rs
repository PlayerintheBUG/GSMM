pub mod interactive;
pub mod commands;

pub use commands::{handle_command, handle_console_input};

use std::path::PathBuf;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "gsmm")]
#[command(author = "PlayerintheBUG")]
#[command(version = "0.2.0")]
#[command(about = "Lightweight Minecraft Server Manager (CLI & Web UI)", long_about = None)]
pub struct Cli {
    /// Directory del server (default: directory corrente)
    #[arg(short, long, global = true)]
    pub dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Inizializza e configura un nuovo server Minecraft
    Init {
        /// Versione di Minecraft (es. 1.21.1)
        #[arg(short = 'v', long)]
        version: Option<String>,

        /// Loader (vanilla, paper, purpur, fabric, neoforge)
        #[arg(short = 'l', long)]
        loader: Option<String>,

        /// RAM massima in MB (es. 4096)
        #[arg(short = 'r', long)]
        ram: Option<u32>,

        /// Lista di mod da installare (separate da virgola)
        #[arg(short = 'm', long)]
        mods: Option<String>,

        /// Nome identificativo per questo server (usato nel registro globale)
        #[arg(short = 'n', long)]
        name: Option<String>,
    },

    /// Avvia il server Minecraft in console interattiva
    Start {
        /// RAM massima in MB per questo avvio (opzionale)
        #[arg(short = 'r', long)]
        ram: Option<u32>,
    },

    /// Arresta il server in esecuzione
    Stop,

    /// Mostra lo stato del server e la configurazione
    Status,

    /// Aggiorna la versione o il modloader mantenendo intatto il mondo
    Upgrade {
        /// Nuova versione di Minecraft (es. 1.21.4)
        #[arg(short = 'v', long)]
        version: Option<String>,

        /// Nuovo loader (vanilla, paper, fabric, neoforge, purpur)
        #[arg(short = 'l', long)]
        loader: Option<String>,

        /// Crea un backup del mondo prima di procedere
        #[arg(short = 'b', long)]
        backup: Option<bool>,
    },

    /// Crea un archivio di backup del mondo e delle configurazioni
    Backup,

    /// Gestione delle mod da Modrinth
    #[command(subcommand)]
    Mod(ModCommands),

    /// Diagnostica di rete: IP pubblico, LAN e stato porta
    Check {
        /// Porta da verificare (default: dalla config o 25565)
        #[arg(short, long)]
        port: Option<u16>,
    },

    /// Avvia l'interfaccia grafica Web
    Web {
        /// Porta su cui avviare il server web (default: 8080)
        #[arg(short, long, default_value = "8080")]
        port: u16,

        /// Non aprire automaticamente il browser
        #[arg(long)]
        no_open: bool,
    },

    /// Gestione del registro globale dei server GSMM
    #[command(subcommand)]
    Server(ServerCommands),

    /// Installa/rimuovi GSMM come servizio di sistema (systemd/launchd/Windows)
    #[command(subcommand)]
    Service(ServiceCommands),
}

#[derive(Subcommand, Debug)]
pub enum ServerCommands {
    /// Elenca tutti i server registrati nel registro globale
    List,

    /// Aggiunge un server esistente al registro globale
    Add {
        /// Nome identificativo del server (es. survival, creative)
        name: String,
        /// Percorso alla cartella del server (default: directory corrente)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },

    /// Rimuove un server dal registro globale (non cancella i file)
    Remove {
        /// Nome del server da rimuovere
        name: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ServiceCommands {
    /// Installa GSMM come servizio di avvio automatico del sistema
    Install {
        /// Nome del servizio (es. survival). Default: nome nella config del server corrente
        #[arg(short, long)]
        name: Option<String>,
        /// Percorso al binario gsmm (default: binario corrente)
        #[arg(short, long)]
        bin: Option<String>,
    },

    /// Rimuove il servizio di sistema installato
    Uninstall {
        /// Nome del servizio da rimuovere
        name: String,
    },

    /// Mostra lo stato del servizio di sistema
    Status {
        /// Nome del servizio
        name: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ModCommands {
    /// Cerca mod su Modrinth
    Search {
        /// Termine di ricerca
        query: String,
        /// Limite risultati (default 5)
        #[arg(short, long, default_value = "5")]
        limit: u32,
    },

    /// Installa una o più mod da Modrinth
    Add {
        /// Nome o slug della mod (es. 'lithium', 'sodium')
        slugs: Vec<String>,
    },

    /// Elenca le mod attualmente installate
    List,

    /// Rimuove una mod installata
    Remove {
        /// Nome del file .jar o slug della mod
        filename: String,
    },
}
