use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStats {
    pub running: bool,
    pub pid: Option<u32>,
    pub cpu_percent: f32,
    pub memory_mb: u64,
    pub memory_limit_mb: Option<u32>,
    pub memory_percent: Option<f32>,
    pub uptime_seconds: u64,
}

impl ServerStats {
    pub fn offline(memory_limit_mb: Option<u32>) -> Self {
        Self {
            running: false,
            pid: None,
            cpu_percent: 0.0,
            memory_mb: 0,
            memory_limit_mb,
            memory_percent: None,
            uptime_seconds: 0,
        }
    }

    pub fn format_uptime(&self) -> String {
        let s = self.uptime_seconds;
        if s < 60 {
            format!("{}s", s)
        } else if s < 3600 {
            format!("{}m {}s", s / 60, s % 60)
        } else {
            format!("{}h {}m", s / 3600, (s % 3600) / 60)
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProcessMetrics {
    pub memory_rss_mb: u64,
    pub cpu_percent: f32,
}

#[derive(Debug, Clone)]
pub struct CpuSample {
    pub timestamp: std::time::Instant,
    pub total_ticks: u64,
}

// ─── Implementazione Linux (lettura diretta ultraleggera da /proc) ────────────
#[cfg(target_os = "linux")]
fn read_linux_memory_rss(pid: u32) -> u64 {
    let path = format!("/proc/{}/status", pid);
    if let Ok(content) = std::fs::read_to_string(path) {
        for line in content.lines() {
            if line.starts_with("VmRSS:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(kb) = parts[1].parse::<u64>() {
                        return kb / 1024;
                    }
                }
            }
        }
    }
    0
}

#[cfg(target_os = "linux")]
fn read_linux_cpu_ticks(pid: u32) -> Option<u64> {
    let path = format!("/proc/{}/stat", pid);
    let content = std::fs::read_to_string(path).ok()?;
    // Il nome del processo è racchiuso tra parentesi: es. 1234 (java) S ...
    let r_paren = content.rfind(')')?;
    let rest = &content[r_paren + 1..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // Campi relativi dopo ')':
    // campo 3 (state)  -> indice 0
    // campo 14 (utime) -> indice 11
    // campo 15 (stime) -> indice 12
    if fields.len() > 12 {
        let utime = fields[11].parse::<u64>().ok()?;
        let stime = fields[12].parse::<u64>().ok()?;
        Some(utime + stime)
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
pub fn read_process_metrics(
    pid: u32,
    last_sample: &mut Option<CpuSample>,
) -> ProcessMetrics {
    let memory_rss_mb = read_linux_memory_rss(pid);
    let current_ticks = read_linux_cpu_ticks(pid);

    let mut cpu_percent = 0.0;
    let now = std::time::Instant::now();

    if let Some(ticks) = current_ticks {
        if let Some(prev) = last_sample {
            let elapsed_sec = now.duration_since(prev.timestamp).as_secs_f32();
            if elapsed_sec > 0.05 && ticks >= prev.total_ticks {
                let delta_ticks = (ticks - prev.total_ticks) as f32;
                // Standard CLK_TCK su Linux è 100 ticks al secondo
                let clk_tck = 100.0;
                let cpu_time_sec = delta_ticks / clk_tck;
                cpu_percent = (cpu_time_sec / elapsed_sec) * 100.0;
                if cpu_percent < 0.0 {
                    cpu_percent = 0.0;
                }
            }
        }
        *last_sample = Some(CpuSample {
            timestamp: now,
            total_ticks: ticks,
        });
    }

    ProcessMetrics {
        memory_rss_mb,
        cpu_percent: (cpu_percent * 10.0).round() / 10.0,
    }
}

// ─── Fallback per sistemi non-Linux (Windows, macOS, ecc.) ────────────────────
#[cfg(not(target_os = "linux"))]
pub fn read_process_metrics(
    _pid: u32,
    _last_sample: &mut Option<CpuSample>,
) -> ProcessMetrics {
    ProcessMetrics {
        memory_rss_mb: 0,
        cpu_percent: 0.0,
    }
}
