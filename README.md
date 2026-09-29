# ⛏️ GSMM (Game & Server Minecraft Manager)

**GSMM** è un gestore di server Minecraft locale **ultra-leggero**, compatto e multipiattaforma (Linux, Windows, macOS) scritto in **Rust**.

È concepito per consumare il **minimo assoluto di risorse** (~10 MB di RAM, 0% CPU in idle) e funzionare sia tramite **CLI interattiva** (perfetta per server headless Linux) che tramite una **Web UI moderna incorporata** nello stesso binario.

---

## 🚀 Caratteristiche Principali

* **Download Automatico in 1 Click**:
  * **Vanilla**: Mojang Version Manifest (`piston-meta`).
  * **Paper / Purpur**: PaperMC API v2 con download dell'ultima build.
  * **Fabric / Quilt**: Fabric Meta API con download diretto del server launcher jar.
  * **NeoForge / Forge**: Download installer e configurazione automatizzata.
* **Integrazione Modrinth (API v2)**:
  * Cerca mod direttamente da terminale o browser.
  * Risoluzione automatica compatibilità versione Minecraft & loader.
  * Risoluzione e download automatico delle **dipendenze richieste** (es. *Fabric API*).
* **Diagnostica Rete & Port Forwarding**:
  * Rilevamento istantaneo dell'IP locale (LAN) e dell'IP pubblico (WAN).
  * Verifica dello stato di apertura porte (`25565`).
  * Generazione dell'indirizzo pronto da inviare agli amici.
* **Doppia Interfaccia**:
  * **CLI / Terminale**: Wizard a frecce direzionali + comandi rapidi.
  * **Web UI**: Dashboard dark theme con console live in tempo reale (WebSocket), gestione mod e diagnostica.

---

## 📦 Installazione / Compilazione


```bash
# Per compilare 
cargo build --release

# Per renderlo accessibile ovunque (su Linux)
sudo cp target/release/gsmm /usr/local/bin/
```

---

## 🛠️ Guida all'Uso

### 1. Inizializzazione di un nuovo server
Spostati nella cartella in cui desideri creare il server e lancia il wizard interattivo (il binario deve essere presente nella cartella):
```bash
./gsmm init
```
Oppure configuralo direttamente con parametri:
```bash
./gsmm init --version 1.21.1 --loader fabric --ram 4096 --mods "lithium,ferrite-core,chunky"
```

### 2. Avvio del Server
```bash
./gsmm start
```
*I log del server vengono mostrati a schermo. Puoi digitare sia comandi Minecraft che comandi GSMM direttamente nello stesso terminale:*
* **Comandi Minecraft**: `help`, `op nome`, `whitelist add nome`, `stop`
* **Comandi GSMM in tempo reale**:
  * `!check` o `gsmm check` $\rightarrow$ Mostra l'IP pubblico e verifica la porta live
  * `!mod search <nome>` $\rightarrow$ Cerca mod su Modrinth
  * `!mod add <nome>` $\rightarrow$ Scarica mod da Modrinth direttamente in `mods/`
  * `!mod list` $\rightarrow$ Mostra le mod installate
  * `!status` $\rightarrow$ Mostra RAM e info server
  * `!help` $\rightarrow$ Guida ai comandi della console

### 3. Apertura della Web UI
```bash
./gsmm web
```
Apre automaticamente la dashboard nel browser su `http://localhost:8080`. Include le schede per la console live, Modrinth, diagnostica di rete e la nuova scheda **"Impostazioni & Aggiorna"**.

### 4. Aggiornamento Versione / Cambio Loader (Preservando il Mondo)
```bash
# Modalità interattiva (chiede se fare il backup, la nuova versione e il loader)
./gsmm upgrade

# Oppure con parametri diretti
./gsmm upgrade --version 1.21.2 --loader fabric --backup true
```
*Il mondo e tutte le costruzioni in `world/` restano intatti al 100%.*

### 5. Backup Manuale del Mondo
```bash
./gsmm backup
```
*Crea una copia compressa `.tar.gz` di sicurezza del mondo nella cartella `backups/`.*

### 6. Gestione Mod con Modrinth
```bash
# Cerca una mod
./gsmm mod search "voice chat"

# Installa una o più mod
./gsmm mod add simple-voice-chat lithium

# Elenca le mod installate
./gsmm mod list

# Rimuovi una mod
./gsmm mod remove simple-voice-chat
```

### 7. Verifica Rete & Connettività (IP Pubblico)
```bash
./gsmm check
```
Mostra l'indirizzo IP locale per i dispositivi di casa e l'indirizzo pubblico da inviare agli amici.

---

## 📁 Struttura del Progetto

* `src/main.rs`: Entry point CLI.
* `src/core/`: Configurazione server, rilevamento Java cross-platform, gestione del processo Java.
* `src/downloader/`: Client API Mojang, PaperMC, Fabric, NeoForge e Modrinth API v2.
* `src/network/`: Rilevamento IP pubblico/locale e test porte TCP.
* `src/cli/`: Wizard interattivo con menu da tastiera e comandi CLI.
* `src/web/`: Server web Axum con WebSocket e interfaccia embedded.
