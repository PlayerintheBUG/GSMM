pub mod interactive;
pub mod commands;

pub use commands::{handle_command, handle_console_input};

use std::path::PathBuf;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "gsmm")]
#[command(author = "Mattia")]
#[command(version = "0.1.0")]
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
    /// Inizializza e configura un nuovo server in modo interattivo o con flag
    Init {
        /// Versione di Minecraft (es. 1.21.1)
        #[arg(short = 'v', long)]
        version: Option<String>,

        /// Loader (vanilla, paper, fabric, neoforge)
        #[arg(short = 'l', long)]
        loader: Option<String>,

        /// RAM massima in MB (es. 4096)
        #[arg(short = 'r', long)]
        ram: Option<u32>,

        /// Lista di mod da installare (separate da virgola)
        #[arg(short = 'm', long)]
        mods: Option<String>,
    },

    /// Avvia il server Minecraft in console
    Start {
        /// RAM massima in MB per questo avvio (opzionale)
        #[arg(short = 'r', long)]
        ram: Option<u32>,
    },

    /// Arresta il server in esecuzione
    Stop,

    /// Mostra lo stato del server
    Status,

    /// Aggiorna la versione di Minecraft o cambia il modloader mantenendo intatto il mondo
    Upgrade {
        /// Nuova versione di Minecraft (es. 1.21.2)
        #[arg(short = 'v', long)]
        version: Option<String>,

        /// Nuovo loader (vanilla, paper, fabric, neoforge, purpur)
        #[arg(short = 'l', long)]
        loader: Option<String>,

        /// Crea una copia di backup di sicurezza del mondo prima di procedere
        #[arg(short = 'b', long)]
        backup: Option<bool>,
    },

    /// Crea un archivio di backup del mondo e delle configurazioni
    Backup,

    /// Gestione delle mod da Modrinth
    #[command(subcommand)]
    Mod(ModCommands),

    /// Esegue la diagnostica di rete, IP pubblico e porta
    Check {
        /// Porta da verificare (default: letta da config o 25565)
        #[arg(short, long)]
        port: Option<u16>,
    },

    /// Avvia l'interfaccia grafica Web locale
    Web {
        /// Porta su cui avviare il server web (default: 8080)
        #[arg(short, long, default_value = "8080")]
        port: u16,

        /// Non aprire automaticamente il browser
        #[arg(long)]
        no_open: bool,
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

    /// Installa una mod o lista di mod da Modrinth
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
