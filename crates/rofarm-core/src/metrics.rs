use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct ClientSample {
    pub at_ms: i64,
    pub cpu_pct: f32,
    pub mem_mb: u32,
    pub gpu_pct: f32,
    pub vram_mb: u32,
    pub disk_kbps: f32,
    pub net_up_kbps: f32,
    pub net_down_kbps: f32,
    pub fps_est: f32,
    pub threads: u32,
    pub handles: u32,
}

impl ClientSample {
    pub fn at(&self) -> DateTime<Utc> {
        DateTime::from_timestamp_millis(self.at_ms).unwrap_or_else(Utc::now)
    }

    pub fn now(cpu: f32, mem: u32) -> Self {
        Self {
            at_ms: Utc::now().timestamp_millis(),
            cpu_pct: cpu,
            mem_mb: mem,
            ..Default::default()
        }
    }
}

pub const SAMPLE_STRUCT_SIZE: usize = 48;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Series {
    pub capacity: usize,
    samples: VecDeque<ClientSample>,
}

impl Series {
    pub fn new(capacity: usize) -> Self {
        Self { capacity: capacity.max(1), samples: VecDeque::with_capacity(capacity.max(1)) }
    }

    pub fn push(&mut self, sample: ClientSample) {
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = &ClientSample> {
        self.samples.iter()
    }
    pub fn latest(&self) -> Option<&ClientSample> {
        self.samples.back()
    }

    pub fn max_memory(&self) -> u32 {
        self.samples.iter().map(|s| s.mem_mb).max().unwrap_or(0)
    }
    pub fn avg_cpu(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        self.samples.iter().map(|s| s.cpu_pct).sum::<f32>() / self.samples.len() as f32
    }

    pub fn sparkline(&self) -> Vec<u8> {
        let values: Vec<u32> = self.samples.iter().map(|s| s.mem_mb).collect();
        normalise(&values)
    }
}

pub fn normalise(values: &[u32]) -> Vec<u8> {
    if values.is_empty() {
        return Vec::new();
    }
    let min = *values.iter().min().unwrap_or(&0);
    let max = *values.iter().max().unwrap_or(&0);
    if max == min {
        return vec![128; values.len()];
    }
    values
        .iter()
        .map(|v| (((v - min) as f64 / (max - min) as f64) * 255.0).round() as u8)
        .collect()
}

pub fn encode_samples(samples: &[ClientSample]) -> Vec<u8> {
    let mut out = Vec::with_capacity(9 + samples.len() * SAMPLE_STRUCT_SIZE);
    out.extend_from_slice(b"ROFM");
    out.push(1);
    out.extend_from_slice(&(samples.len() as u32).to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.at_ms.to_le_bytes());
        out.extend_from_slice(&s.cpu_pct.to_le_bytes());
        out.extend_from_slice(&s.mem_mb.to_le_bytes());
        out.extend_from_slice(&s.gpu_pct.to_le_bytes());
        out.extend_from_slice(&s.vram_mb.to_le_bytes());
        out.extend_from_slice(&s.disk_kbps.to_le_bytes());
        out.extend_from_slice(&s.net_up_kbps.to_le_bytes());
        out.extend_from_slice(&s.net_down_kbps.to_le_bytes());
        out.extend_from_slice(&s.fps_est.to_le_bytes());
        out.extend_from_slice(&s.threads.to_le_bytes());
        out.extend_from_slice(&s.handles.to_le_bytes());
    }
    out
}

pub fn decode_samples(data: &[u8]) -> Result<Vec<ClientSample>, &'static str> {
    if data.len() < 9 || &data[0..4] != b"ROFM" {
        return Err("bad magic");
    }
    if data[4] != 1 {
        return Err("unsupported version");
    }
    let count = u32::from_le_bytes(data[5..9].try_into().map_err(|_| "bad length")?) as usize;
    if data.len() != 9 + count * SAMPLE_STRUCT_SIZE {
        return Err("length mismatch");
    }
    let mut out = Vec::with_capacity(count);
    let mut o = 9usize;
    for _ in 0..count {
        let f32_at = |offset: usize| f32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
        let at_ms = i64::from_le_bytes(data[o..o + 8].try_into().unwrap());
        let cpu = f32_at(o + 8);
        let mem = u32::from_le_bytes(data[o + 12..o + 16].try_into().unwrap());
        let gpu = f32_at(o + 16);
        let vram = u32::from_le_bytes(data[o + 20..o + 24].try_into().unwrap());
        let disk = f32_at(o + 24);
        let up = f32_at(o + 28);
        let down = f32_at(o + 32);
        let fps = f32_at(o + 36);
        let threads = u32::from_le_bytes(data[o + 40..o + 44].try_into().unwrap());
        let handles = u32::from_le_bytes(data[o + 44..o + 48].try_into().unwrap());
        out.push(ClientSample {
            at_ms,
            cpu_pct: cpu,
            mem_mb: mem,
            gpu_pct: gpu,
            vram_mb: vram,
            disk_kbps: disk,
            net_up_kbps: up,
            net_down_kbps: down,
            fps_est: fps,
            threads,
            handles,
        });
        o += SAMPLE_STRUCT_SIZE;
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct FleetTotals {
    pub clients: u32,
    pub cpu_pct: f32,
    pub mem_mb: u32,
    pub gpu_pct: f32,
    pub net_up_kbps: f32,
    pub net_down_kbps: f32,
    pub total_ram_mb: u32,
    pub ram_used_pct: f32,
    pub alerts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Anomaly {
    MemoryLeak { pct_growth_per_hour: u32 },
    RepeatedDisconnects { count: u32 },
    CpuSaturation { avg_pct: u32 },
    HandleGrowth { per_hour: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemProfile {
    pub logical_cores: u32,
    pub physical_cores: u32,
    pub ram_gb: u32,
    pub vram_gb: u32,
    pub gpu_tier: GpuTier,
    pub is_laptop: bool,
    pub on_battery: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GpuTier {
    Integrated,
    #[default]
    Midrange,
    HighEnd,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapacityEstimate {
    pub recommended_clients: u32,
    pub limiting_factor: String,
    pub per_client_cpu_pct: u32,
    pub per_client_ram_mb: u32,
    pub notes: Vec<String>,
}

pub fn estimate_capacity(system: SystemProfile, observed: Option<ClientSample>) -> CapacityEstimate {
    let per_cpu = observed.map(|s| s.cpu_pct.max(1.0)).unwrap_or(12.0);
    let per_ram = observed.map(|s| s.mem_mb.max(300)).unwrap_or(1100);
    let usable_ram_mb = system.ram_gb.saturating_mul(1024).saturating_sub(2560);
    let cpu_limit = ((system.logical_cores as f32 * 100.0 * 0.75) / per_cpu).floor() as u32;
    let ram_limit = if usable_ram_mb == 0 { 1 } else { usable_ram_mb / per_ram };
    let gpu_limit = match system.gpu_tier {
        GpuTier::Integrated => 3,
        GpuTier::Midrange => 8,
        GpuTier::HighEnd => 16,
    };
    let mut cap = cpu_limit.min(ram_limit).min(gpu_limit).max(1);
    let mut factor = if cap == cpu_limit { "CPU" } else if cap == ram_limit { "RAM" } else { "GPU" };
    if system.is_laptop {
        cap = cap.min(if system.on_battery { 2 } else { 5 }).max(1);
        factor = "laptop thermals";
    }
    CapacityEstimate {
        recommended_clients: cap,
        limiting_factor: factor.into(),
        per_client_cpu_pct: per_cpu.ceil() as u32,
        per_client_ram_mb: per_ram,
        notes: vec![
            "Estimate assumes Roblox graphics at Low and a 30 FPS cap.".into(),
            "Leave headroom: running at the limit makes every client stutter.".into(),
        ],
    }
}

pub fn detect_anomalies(series: &Series, disconnects_last_hour: u32) -> Vec<Anomaly> {
    let mut out = Vec::new();
    if series.len() >= 10 {
        let n = series.len() as f64;
        let t0 = series.iter().next().unwrap().at_ms;
        let xs: Vec<f64> = series.iter().map(|s| (s.at_ms - t0) as f64 / 3_600_000.0).collect();
        let ys: Vec<f64> = series.iter().map(|s| s.mem_mb as f64).collect();
        let mean_x = xs.iter().sum::<f64>() / n;
        let mean_y = ys.iter().sum::<f64>() / n;
        let num: f64 = xs.iter().zip(&ys).map(|(x, y)| (x - mean_x) * (y - mean_y)).sum();
        let den: f64 = xs.iter().map(|x| (x - mean_x).powi(2)).sum();
        if den > 0.0 {
            let slope = num / den;
            let first = ys.first().copied().unwrap_or(1.0).max(1.0);
            let growth = slope / first * 100.0;
            if growth > 8.0 && slope > 20.0 {
                out.push(Anomaly::MemoryLeak { pct_growth_per_hour: growth.round() as u32 });
            }
        }
        let avg_cpu = series.avg_cpu();
        if avg_cpu > 85.0 {
            out.push(Anomaly::CpuSaturation { avg_pct: avg_cpu.round() as u32 });
        }
    }
    if disconnects_last_hour >= 3 {
        out.push(Anomaly::RepeatedDisconnects { count: disconnects_last_hour });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_samples_round_trip() {
        let samples: Vec<ClientSample> = (0..10)
            .map(|i| ClientSample {
                at_ms: 1_700_000_000_000 + i,
                cpu_pct: i as f32 * 1.5,
                mem_mb: 800 + i as u32,
                gpu_pct: 20.0,
                vram_mb: 512,
                disk_kbps: 3.0,
                net_up_kbps: 4.0,
                net_down_kbps: 5.0,
                fps_est: 29.9,
                threads: 42,
                handles: 1000,
            })
            .collect();
        let encoded = encode_samples(&samples);
        assert_eq!(encoded.len(), 9 + samples.len() * SAMPLE_STRUCT_SIZE);
        let decoded = decode_samples(&encoded).unwrap();
        assert_eq!(decoded, samples);
        assert!(decode_samples(&encoded[..encoded.len() - 1]).is_err());
        assert!(decode_samples(b"nope").is_err());
    }

    #[test]
    fn series_is_bounded_and_sparkline_normalises() {
        let mut s = Series::new(5);
        for i in 0..20 {
            s.push(ClientSample::now(10.0, 500 + i));
        }
        assert_eq!(s.len(), 5);
        let oldest = s.iter().next().unwrap();
        assert_eq!(oldest.mem_mb, 515);
        assert_eq!(oldest.cpu_pct, 10.0);
        let spark = s.sparkline();
        assert_eq!(spark.len(), 5);
        assert_eq!(*spark.last().unwrap(), 255);
        assert_eq!(spark[0], 0);
    }

    #[test]
    fn capacity_estimate_is_sane() {
        let system = SystemProfile {
            logical_cores: 16,
            physical_cores: 8,
            ram_gb: 32,
            vram_gb: 8,
            gpu_tier: GpuTier::Midrange,
            is_laptop: false,
            on_battery: false,
        };
        let e = estimate_capacity(system, None);
        assert!(e.recommended_clients >= 3 && e.recommended_clients <= 16, "{e:?}");
        let laptop = estimate_capacity(SystemProfile { is_laptop: true, on_battery: true, ..system }, None);
        assert!(laptop.recommended_clients <= 2);
    }

    #[test]
    fn leak_detection_finds_growth() {
        let mut series = Series::new(100);
        let now = Utc::now().timestamp_millis();
        for i in 0..60 {
            let mut sample = ClientSample::now(10.0, 500 + (i as u32 * 8));
            sample.at_ms = now + i * 60_000;
            series.push(sample);
        }
        let anomalies = detect_anomalies(&series, 0);
        assert!(anomalies.iter().any(|a| matches!(a, Anomaly::MemoryLeak { .. })));
        assert!(detect_anomalies(&series, 4).iter().any(|a| matches!(a, Anomaly::RepeatedDisconnects { .. })));
    }
}
