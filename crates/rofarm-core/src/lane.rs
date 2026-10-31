use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::ids::{AccountId, LaneId, LaunchProfileId};
use crate::schedule::Schedule;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PriorityClass {
    Idle,
    BelowNormal,
    #[default]
    Normal,
    AboveNormal,
    High,
}

impl PriorityClass {
    pub fn win32_value(&self) -> u32 {
        match self {
            PriorityClass::Idle => 0x40,
            PriorityClass::BelowNormal => 0x4000,
            PriorityClass::Normal => 0x20,
            PriorityClass::AboveNormal => 0x8000,
            PriorityClass::High => 0x80,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IoPriority {
    VeryLow,
    Low,
    #[default]
    Normal,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GpuPreference {
    #[default]
    Auto,
    PowerSaving,
    HighPerformance,
}

impl GpuPreference {
    pub fn registry_value(&self) -> &'static str {
        match self {
            GpuPreference::Auto => "0",
            GpuPreference::PowerSaving => "1",
            GpuPreference::HighPerformance => "2",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GraphicsPreset {
    Lowest,
    Low,
    #[default]
    Balanced,
    High,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub cpu_percent: Option<u8>,
    pub memory_mb: Option<u32>,
    pub trim_memory: bool,
    pub eco_qos: bool,
    pub priority: PriorityClass,
    pub io_priority: IoPriority,
    pub gpu: GpuPreference,
    pub fps_cap: Option<u16>,
    pub graphics: GraphicsPreset,
    pub upload_kbps: Option<u32>,
    pub cores: Vec<usize>,
    pub affinity_mask: Option<u64>,
    pub auto_throttle_memory: bool,
    pub background_fps_cap: Option<u16>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu_percent: Some(35),
            memory_mb: None,
            trim_memory: false,
            eco_qos: false,
            priority: PriorityClass::BelowNormal,
            io_priority: IoPriority::Low,
            gpu: GpuPreference::Auto,
            fps_cap: Some(30),
            graphics: GraphicsPreset::Lowest,
            upload_kbps: None,
            cores: Vec::new(),
            affinity_mask: None,
            auto_throttle_memory: false,
            background_fps_cap: Some(15),
        }
    }
}

impl ResourceLimits {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(cpu) = self.cpu_percent {
            if !(1..=100).contains(&cpu) {
                return Err("cpu_percent must be 1-100".into());
            }
        }
        if let Some(mem) = self.memory_mb {
            if !(256..=65_536).contains(&mem) {
                return Err("memory_mb must be 256-65536".into());
            }
        }
        if let Some(fps) = self.fps_cap {
            if !(1..=240).contains(&fps) {
                return Err("fps_cap must be 1-240".into());
            }
        }
        if self.cores.iter().any(|&c| c > 255) {
            return Err("core index out of range".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuietHours {
    pub start_minute: u32,
    pub end_minute: u32,
    pub weekends_only: bool,
}

impl QuietHours {
    pub fn contains_local(&self, local_minute: u32, is_weekend: bool) -> bool {
        if self.weekends_only && !is_weekend {
            return false;
        }
        if self.start_minute <= self.end_minute {
            (self.start_minute..self.end_minute).contains(&local_minute)
        } else {
            local_minute >= self.start_minute || local_minute < self.end_minute
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlertPolicy {
    pub enabled: bool,
    pub mute: bool,
    pub quiet_hours: Option<QuietHours>,
    pub rate_limit_per_hour: u32,
    pub events: BTreeSet<AlertKind>,
}

impl Default for AlertPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            mute: false,
            quiet_hours: None,
            rate_limit_per_hour: 12,
            events: AlertKind::default_set(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertKind {
    ClientCrashed,
    ClientDisconnected,
    AccountNeedsLogin,
    RobuxChanged,
    FriendOnline,
    HighMemory,
    AutomationFailed,
    UpdateAvailable,
    BackupFailed,
    UnlockFailed,
}

impl AlertKind {
    pub fn default_set() -> BTreeSet<Self> {
        [
            AlertKind::ClientCrashed,
            AlertKind::AccountNeedsLogin,
            AlertKind::AutomationFailed,
            AlertKind::UnlockFailed,
        ]
        .into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RotationOrder {
    #[default]
    RoundRobin,
    LeastPlaytime,
    Healthiest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rotation {
    pub keep_running: u32,
    pub max_session_minutes: u32,
    pub swap_cooldown_seconds: u32,
    pub order: RotationOrder,
}

impl Default for Rotation {
    fn default() -> Self {
        Self { keep_running: 3, max_session_minutes: 120, swap_cooldown_seconds: 120, order: RotationOrder::RoundRobin }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AntiAfkMode {
    Foreground,
    Background,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AntiAfkConfig {
    pub enabled: bool,
    pub mode: AntiAfkMode,
    pub interval_seconds: u32,
    pub jitter_percent: u8,
    pub opted_out_accounts: BTreeSet<AccountId>,
}

impl Default for AntiAfkConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: AntiAfkMode::Background,
            interval_seconds: 300,
            jitter_percent: 25,
            opted_out_accounts: BTreeSet::new(),
        }
    }
}

impl AntiAfkConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(30..=3600).contains(&self.interval_seconds) {
            return Err("anti-AFK interval must be 30-3600 seconds".into());
        }
        if self.jitter_percent > 50 {
            return Err("anti-AFK jitter must be <= 50%".into());
        }
        Ok(())
    }

    pub fn jittered_interval(&self, tick: u64) -> u32 {
        if self.jitter_percent == 0 {
            return self.interval_seconds;
        }
        let span = (self.interval_seconds as u64 * self.jitter_percent as u64) / 100;
        if span == 0 {
            return self.interval_seconds;
        }
        let offset = (tick.wrapping_mul(6364136223846793005).rotate_left(17) % (span * 2 + 1)) as i64
            - span as i64;
        (self.interval_seconds as i64 + offset).max(30) as u32
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LaneRules {
    pub default_target: Option<String>,
    pub default_profile: Option<LaunchProfileId>,
    pub limits: ResourceLimits,
    pub alerts: AlertPolicy,
    pub schedule: Option<Schedule>,
    pub rotation: Option<Rotation>,
    pub max_running: Option<u32>,
    pub anti_afk: AntiAfkConfig,
    pub stop_on_lock: bool,
    pub allowed_to_stay_disconnected: bool,
    pub auto_rejoin: bool,
    pub restart_on_crash: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lane {
    pub id: LaneId,
    pub name: String,
    pub color: String,
    pub order: u32,
    pub builtin: bool,
    pub rules: LaneRules,
    pub created_at: DateTime<Utc>,
}

impl LaneRules {
    pub fn validate(&self) -> Result<(), String> {
        self.limits.validate()?;
        self.anti_afk.validate()?;
        if let Some(schedule) = &self.schedule {
            schedule.validate().map_err(|e| e.to_string())?;
        }
        if let Some(rotation) = &self.rotation {
            if rotation.keep_running == 0 || rotation.keep_running > 50 {
                return Err("rotation keep_running must be 1-50".into());
            }
            if rotation.max_session_minutes < 5 {
                return Err("rotation sessions must be at least 5 minutes".into());
            }
        }
        if let Some(max) = self.max_running {
            if max == 0 || max > 100 {
                return Err("max running clients must be 1-100".into());
            }
        }
        if let Some(target) = &self.default_target {
            crate::launch::parse_target(target).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

impl Lane {
    pub fn new(name: impl Into<String>, order: u32) -> Self {
        Self {
            id: LaneId::new(),
            name: name.into(),
            color: "#6c8cff".into(),
            order,
            builtin: false,
            rules: LaneRules::default(),
            created_at: Utc::now(),
        }
    }

    pub fn builtin(id: LaneId, name: &str, color: &str, order: u32) -> Self {
        Self { id, name: name.into(), color: color.into(), order, builtin: true, rules: LaneRules::default(), created_at: Utc::now() }
    }

    pub fn seed() -> Vec<Self> {
        use crate::ids::builtin;
        vec![
            Lane::builtin(builtin::farming(), "Farming", "#5b8cff", 0),
            Lane::builtin(builtin::idle(), "Idle", "#8892a6", 1),
            Lane::builtin(builtin::trading(), "Trading", "#e0a44a", 2),
            Lane::builtin(builtin::testing(), "Testing", "#b07cff", 3),
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResourceProfile {
    Eco,
    #[default]
    Balanced,
    Performance,
    Custom,
}

impl ResourceProfile {
    pub fn limits(&self) -> ResourceLimits {
        match self {
            ResourceProfile::Eco => ResourceLimits {
                cpu_percent: Some(20),
                memory_mb: Some(1400),
                trim_memory: true,
                eco_qos: true,
                priority: PriorityClass::Idle,
                io_priority: IoPriority::VeryLow,
                gpu: GpuPreference::PowerSaving,
                fps_cap: Some(15),
                graphics: GraphicsPreset::Lowest,
                upload_kbps: Some(128),
                cores: vec![],
                affinity_mask: None,
                auto_throttle_memory: true,
                background_fps_cap: Some(10),
            },
            ResourceProfile::Balanced => ResourceLimits {
                cpu_percent: Some(35),
                memory_mb: Some(2500),
                trim_memory: false,
                eco_qos: true,
                priority: PriorityClass::BelowNormal,
                io_priority: IoPriority::Low,
                gpu: GpuPreference::Auto,
                fps_cap: Some(30),
                graphics: GraphicsPreset::Low,
                upload_kbps: None,
                cores: vec![],
                affinity_mask: None,
                auto_throttle_memory: false,
                background_fps_cap: Some(15),
            },
            ResourceProfile::Performance => ResourceLimits {
                cpu_percent: Some(60),
                memory_mb: None,
                trim_memory: false,
                eco_qos: false,
                priority: PriorityClass::Normal,
                io_priority: IoPriority::Normal,
                gpu: GpuPreference::HighPerformance,
                fps_cap: Some(60),
                graphics: GraphicsPreset::Balanced,
                upload_kbps: None,
                cores: vec![],
                affinity_mask: None,
                auto_throttle_memory: false,
                background_fps_cap: Some(30),
            },
            ResourceProfile::Custom => ResourceLimits::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lane_seed_is_unique_and_ordered() {
        let lanes = Lane::seed();
        assert_eq!(lanes.len(), 4);
        let ids: BTreeSet<_> = lanes.iter().map(|l| l.id).collect();
        assert_eq!(ids.len(), 4);
        assert!(lanes.iter().all(|l| l.builtin));
    }

    #[test]
    fn quiet_hours_handles_overnight_and_weekends() {
        let q = QuietHours { start_minute: 22 * 60, end_minute: 6 * 60, weekends_only: false };
        assert!(q.contains_local(23 * 60, false));
        assert!(q.contains_local(5 * 60, true));
        assert!(!q.contains_local(12 * 60, false));
        let weekend = QuietHours { start_minute: 0, end_minute: 60, weekends_only: true };
        assert!(weekend.contains_local(30, true));
        assert!(!weekend.contains_local(30, false));
    }

    #[test]
    fn limits_validate_ranges() {
        let mut limits = ResourceLimits::default();
        assert!(limits.validate().is_ok());
        limits.cpu_percent = Some(0);
        assert!(limits.validate().is_err());
        limits.cpu_percent = Some(50);
        limits.memory_mb = Some(10);
        assert!(limits.validate().is_err());
        limits.memory_mb = Some(1024);
        limits.fps_cap = Some(500);
        assert!(limits.validate().is_err());
    }

    #[test]
    fn profiles_have_expected_shapes() {
        assert!(ResourceProfile::Eco.limits().eco_qos);
        assert!(!ResourceProfile::Performance.limits().eco_qos);
        assert_eq!(ResourceProfile::Eco.limits().fps_cap, Some(15));
        assert!(ResourceProfile::Performance.limits().cpu_percent.unwrap() > 35);
    }

    #[test]
    fn anti_afk_jitter_stays_in_bounds() {
        let cfg = AntiAfkConfig { interval_seconds: 300, jitter_percent: 25, ..Default::default() };
        for tick in 0..1000 {
            let v = cfg.jittered_interval(tick);
            assert!((225..=375).contains(&v), "tick {tick} produced {v}");
        }
    }
}
