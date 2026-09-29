use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Context, Result};
use colored::*;

#[derive(Debug, Clone)]
pub struct JavaInfo {
    pub path: PathBuf,
    pub major_version: u32,
    pub version_string: String,
}

pub fn get_recommended_java_version(mc_version: &str) -> u32 {
    let parts: Vec<&str> = mc_version.split('.').collect();
    if parts.len() >= 2 {
        if let Ok(minor) = parts[1].parse::<u32>() {
            if minor < 17 {
                return 8;
            } else if minor < 20 {
                return 17;
            } else if minor == 20 {
                // 1.20 - 1.20.4 use Java 17, 1.20.5+ use Java 21
                if parts.len() >= 3 {
                    if let Ok(patch) = parts[2].parse::<u32>() {
                        if patch >= 5 {
                            return 21;
                        }
                    }
                }
                return 17;
            } else {
                // 1.21+
                return 21;
            }
        }
    }
    21
}

pub fn detect_java(custom_path: Option<&str>) -> Result<JavaInfo> {
    let candidate_paths = if let Some(p) = custom_path {
        vec![PathBuf::from(p)]
    } else {
        find_java_candidates()
    };

    if candidate_paths.is_empty() {
        anyhow::bail!("Nessun eseguibile Java trovato nel sistema! Installa un JDK (Java 17 o 21).");
    }

    for path in &candidate_paths {
        if let Ok(info) = check_java_executable(path) {
            return Ok(info);
        }
    }

    anyhow::bail!("Nessuna installazione valida di Java trovata tra i candidati: {:?}", candidate_paths);
}

fn find_java_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // 1. Check JAVA_HOME environment variable
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let home_path = PathBuf::from(java_home);
        #[cfg(target_os = "windows")]
        let exe = home_path.join("bin").join("java.exe");
        #[cfg(not(target_os = "windows"))]
        let exe = home_path.join("bin").join("java");

        if exe.exists() {
            paths.push(exe);
        }
    }

    // 2. Check standard 'java' in PATH
    #[cfg(target_os = "windows")]
    let default_name = "java.exe";
    #[cfg(not(target_os = "windows"))]
    let default_name = "java";

    paths.push(PathBuf::from(default_name));

    // 3. Common platform search paths
    #[cfg(target_os = "windows")]
    {
        for root in &["C:\\Program Files\\Java", "C:\\Program Files\\Eclipse Adoptium", "C:\\Program Files\\Microsoft"] {
            let root_path = Path::new(root);
            if let Ok(entries) = std::fs::read_dir(root_path) {
                for entry in entries.flatten() {
                    let exe = entry.path().join("bin").join("java.exe");
                    if exe.exists() {
                        paths.push(exe);
                    }
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let linux_jvm_dirs = ["/usr/lib/jvm", "/usr/java"];
        for dir in &linux_jvm_dirs {
            let root_path = Path::new(dir);
            if let Ok(entries) = std::fs::read_dir(root_path) {
                for entry in entries.flatten() {
                    let exe = entry.path().join("bin").join("java");
                    if exe.exists() {
                        paths.push(exe);
                    }
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let macos_jvm = "/Library/Java/JavaVirtualMachines";
        let root_path = Path::new(macos_jvm);
        if let Ok(entries) = std::fs::read_dir(root_path) {
            for entry in entries.flatten() {
                let exe = entry.path().join("Contents/Home/bin/java");
                if exe.exists() {
                    paths.push(exe);
                }
            }
        }
    }

    paths
}

pub fn check_java_executable(path: &Path) -> Result<JavaInfo> {
    let output = Command::new(path)
        .arg("-version")
        .output()
        .with_context(|| format!("Impossibile eseguire '{}'", path.display()))?;

    // java -version prints to stderr
    let output_str = String::from_utf8_lossy(&output.stderr).to_string()
        + &String::from_utf8_lossy(&output.stdout);

    let (major, ver_str) = parse_java_version(&output_str)?;

    Ok(JavaInfo {
        path: path.to_path_buf(),
        major_version: major,
        version_string: ver_str,
    })
}

fn parse_java_version(output: &str) -> Result<(u32, String)> {
    // Example lines:
    // openjdk version "21.0.2" 2024-01-16
    // java version "1.8.0_291"
    let re = regex::Regex::new(r#"(?:version|openjdk)\s+"?([0-9]+(?:\.[0-9]+)*)"#)?;
    if let Some(caps) = re.captures(output) {
        if let Some(ver_match) = caps.get(1) {
            let ver_str = ver_match.as_str();
            let major = if ver_str.starts_with("1.") {
                let parts: Vec<&str> = ver_str.split('.').collect();
                parts.get(1).and_then(|p| p.parse::<u32>().ok()).unwrap_or(8)
            } else {
                let parts: Vec<&str> = ver_str.split('.').collect();
                parts.first().and_then(|p| p.parse::<u32>().ok()).unwrap_or(21)
            };
            return Ok((major, ver_str.to_string()));
        }
    }

    Ok((21, "Sconosciuta (default 21)".to_string()))
}

pub fn validate_java_compatibility(java: &JavaInfo, mc_version: &str) {
    let recommended = get_recommended_java_version(mc_version);
    if java.major_version < recommended {
        println!(
            "{}",
            format!(
                "⚠️  ATTENZIONE: La versione Minecraft {} richiede Java {}, ma è stato rilevato Java {} ({}). Il server potrebbe non avviarsi!",
                mc_version, recommended, java.major_version, java.path.display()
            ).yellow().bold()
        );
    } else {
        println!(
            "{}",
            format!(
                "✓ Rilevato Java {} ({}) compatibile con MC {}",
                java.major_version, java.version_string, mc_version
            ).green()
        );
    }
}
