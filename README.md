# ⛏️ GSMM

**GSMM** è un gestore di server Minecraft **ultra-leggero**, modulare e multipiattaforma (Linux, Windows, macOS) scritto in **Rust**.

Progettato per garantire il **minimo consumo di risorse** (~10 MB di RAM, 0% CPU in idle), include sia una potente **CLI interattiva** (ideale per ambienti headless e VPS) sia una **Web UI moderna embedded** in un unico binario autonomo.

---

## 🚀 Caratteristiche Principali

* **Multi-Server Globale**:
  * Registro centralizzato (`~/.config/gsmm/registry.json`) per gestire più server contemporaneamente.
  * Selezione e controllo simultaneo di più istanze dalla stessa Web UI o tramite CLI.
* **Monitor Risorse Integrato (Ultra-Leggero)**:
  * Monitoraggio in tempo reale di **CPU %**, **RAM fisica reale (RSS)** rispetto al limite configurato, e **Uptime**.
  * Lettura diretta a bassissimo overhead via `/proc` su Linux con fallback compatibile per altri sistemi operativi.
  * Visualizzazione in console live e con mini-barre dinamiche nella Web UI.
* **Download & Setup Automatico in 1-Click**:
  * **Vanilla**: Mojang Version Manifest (`piston-meta`).
  * **Paper / Purpur**: PaperMC API v2 con download dell'ultima build stabile.
  * **Fabric / Quilt**: Fabric Meta API con installazione del server launcher jar.
  * **NeoForge / Forge**: Download installer e configurazione automatizzata.
* **Integrazione Modrinth (API v2)**:
  * Ricerca e download di mod direttamente da terminale o browser.
  * Risoluzione automatica di versione, loader e **dipendenze richieste** (es. *Fabric API*).
* **Sicurezza & Web UI Protetta**:
  * Token di autenticazione monouso generato ad ogni avvio della Web UI e mostrato nel terminale.
  * Blocco a **dispositivo singolo** (1 sola sessione attiva per volta via IP).
  * Gestione completa da interfaccia: console live WebSocket, mod, diagnostica e impostazioni.
* **Diagnostica Rete & Port Forwarding**:
  * Rilevamento istantaneo dell'IP locale (LAN) e dell'IP pubblico (WAN).
  * Verifica dello stato di ascolto locale della porta e istruzioni per il port forwarding.
* **Upgrade Senza Perdita Dati**:
  * Aggiorna la versione di Minecraft o cambia loader mantenendo il mondo (`world/`) intatto al 100%.

---

## 📦 Installazione e Compilazione

### Prerequisiti
* [Rust & Cargo](https://rustup.rs/) (1.75+)
* Java (compatibile con la versione di Minecraft scelta)

### Compilazione dal sorgente
```bash
# Clona il repository
git clone https://github.com/PlayerintheBUG/GSMM.git
cd GSMM

# Compila la build ottimizzata
cargo build --release

# (Opzionale su Linux/macOS) Installa il binario a livello globale
sudo cp target/release/gsmm /usr/local/bin/
```

---

## 🛠️ Guida ai Comandi CLI

### 1. Inizializzazione Server (`init`)
Crea e configura una nuova istanza di Minecraft:
```bash
# Modalità wizard interattivo con menu a frecce
gsmm init

# Oppure configurazione rapida diretta con nome per il registro
gsmm init -v 1.21.1 -l fabric -r 4096 -n survival -m "lithium,ferrite-core"
```

### 2. Gestione Multi-Server (`server`)
Gestisci le istanze registrate nel registro globale:
```bash
# Elenca tutti i server registrati e il loro stato
gsmm server list

# Aggiunge un server esistente al registro (dalla cartella corrente o specificando il percorso)
gsmm server add survival
gsmm server add creative --path /home/utente/minecraft/creative

# Rimuove un server dal registro (i file non vengono eliminati)
gsmm server remove creative
```

### 3. Avvio e Console Live (`start`)
```bash
# Avvia il server nella cartella corrente
gsmm start

# Oppure specificando un limite temporaneo di RAM (MB)
gsmm start --ram 8192
```

Durante l'esecuzione, la console accetta sia comandi Minecraft che comandi speciali GSMM:
* **Comandi Minecraft**: `help`, `op nome`, `whitelist add nome`, `stop`
* **Comandi GSMM in tempo reale**:
  * `!status` $\rightarrow$ Mostra PID, CPU %, RAM reale usata su totale configurato e Uptime.
  * `!check` $\rightarrow$ Mostra IP LAN, IP Pubblico e diagnostica porta.
  * `!backup` $\rightarrow$ Esegue un backup compresso istantaneo del mondo.
  * `!mod search <query>` $\rightarrow$ Cerca mod su Modrinth.
  * `!mod add <slug>` $\rightarrow$ Scarica e installa mod al volo in `mods/`.
  * `!mod list` $\rightarrow$ Elenca le mod attualmente presenti.
  * `!mod remove <file>` $\rightarrow$ Rimuove una mod.
  * `!help` $\rightarrow$ Mostra l'elenco dei comandi GSMM disponibili.

### 4. Interfaccia Grafica Web (`web`)
```bash
# Avvia la Web UI sulla porta predefinita (8080)
gsmm web

# Specifica una porta personalizzata e disabilita l'apertura automatica del browser
gsmm web --port 3000 --no-open
```
* All'avvio viene stampato a terminale un **Token di sicurezza**.
* Apri il browser, inserisci il token per autenticarti.
* Accesso limitato a **1 solo dispositivo alla volta**.
* Dalla Web UI puoi controllare tutti i server registrati, vedere console live, installare mod, monitorare le risorse e aggiornare le impostazioni.

### 5. Installazione come Servizio di Sistema (`service`)
Rende l'avvio del server automatico all'accensione della macchina:
```bash
# Installa come servizio (systemd su Linux, launchd su macOS, scheduler su Windows)
gsmm service install

# Con nome personalizzato
gsmm service install --name survival

# Controlla lo stato o disinstalla il servizio
gsmm service status survival
gsmm service uninstall survival
```

### 6. Aggiornamento Versione & Cambio Loader (`upgrade`)
```bash
# Wizard interattivo
gsmm upgrade

# Diretto da riga di comando
gsmm upgrade --version 1.21.4 --loader fabric --backup true
```
*Scarica i nuovi file e aggiorna la configurazione mantenendo inalterata la cartella `world/`.*

### 7. Backup del Mondo (`backup`)
```bash
gsmm backup
```
*Crea un archivio `.tar.gz` compresso contenente il mondo e le configurazioni nella cartella `backups/`.*

### 8. Gestione Mod da CLI (`mod`)
```bash
gsmm mod search "voice chat"       # Cerca mod compatibili su Modrinth
gsmm mod add simple-voice-chat     # Installa con risoluzione automatica delle dipendenze
gsmm mod list                      # Elenca mod installate in mods/
gsmm mod remove simple-voice-chat  # Rimuove una mod
```

### 9. Verifica di Rete (`check`)
```bash
gsmm check
gsmm check --port 25565
```
*Fornisce gli indirizzi IP (LAN e WAN) formattati e pronti da condividere con altri giocatori.*

---

## 📁 Struttura del Progetto

```text
GSMM/
├── src/
│   ├── main.rs            # Entry point dell'applicazione
│   ├── cli/               # Parser argomenti clap, comandi e wizard interattivo
│   ├── core/
│   │   ├── config.rs      # Gestione file di configurazione (.gsmm.json)
│   │   ├── java.rs        # Rilevamento e validazione versioni Java
│   │   ├── process.rs     # Gestore del processo Java e cattura I/O console
│   │   ├── metrics.rs     # Monitor risorse ultra-leggero (CPU, RAM, Uptime)
│   │   └── backup.rs      # Creazione archivi compressi tar.gz
│   ├── downloader/        # Client API Mojang, PaperMC, Fabric, NeoForge e Modrinth
│   ├── network/           # Diagnostica IP LAN/WAN e verifica porte
│   ├── registry/          # Registro globale multi-server (~/.config/gsmm/registry.json)
│   ├── service/           # Gestione servizi di sistema (systemd, launchd, task scheduler)
│   └── web/               # Server Axum, middleware token auth, WebSocket e Web UI embedded
├── Cargo.toml
└── LICENSE
```

---

## 📄 Licenza

Questo progetto è rilasciato sotto licenza **GNU General Public License v3.0 (GPL-3.0)**. Consulta il file [LICENSE](LICENSE) per i dettagli completi.
