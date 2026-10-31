use chrono::{DateTime, Utc};
use rofarm_core::window::Rect;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub parent_pid: u32,
    pub name: String,
    pub executable: Option<PathBuf>,
    pub started_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowId(pub i64);

impl WindowId {
    pub fn from_isize(v: isize) -> Self {
        Self(v as i64)
    }
    pub fn as_isize(self) -> isize {
        self.0 as isize
    }
    pub fn is_null(&self) -> bool {
        self.0 == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: WindowId,
    pub pid: u32,
    pub title: String,
    pub class_name: String,
    pub rect: Rect,
    pub visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct ProcessMetrics {
    pub pid: u32,
    pub cpu_pct: f32,
    pub mem_mb: u32,
    pub thread_count: u32,
    pub handle_count: u32,
    pub gpu_pct: Option<f32>,
    pub vram_mb: Option<u32>,
    pub disk_kbps: Option<f32>,
    pub net_up_kbps: Option<f32>,
    pub net_down_kbps: Option<f32>,
    pub exited: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct SystemMetrics {
    pub cpu_pct: f32,
    pub mem_total_mb: u32,
    pub mem_used_mb: u32,
    pub gpu_pct: Option<f32>,
    pub net_up_kbps: Option<f32>,
    pub net_down_kbps: Option<f32>,
    pub battery_percent: Option<u8>,
    pub on_battery: bool,
    pub session_locked: bool,
    pub logical_cores: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobLimits {
    pub cpu_percent: Option<u8>,
    pub memory_mb: Option<u32>,
    pub kill_on_close: bool,
}

impl Default for JobLimits {
    fn default() -> Self {
        Self { cpu_percent: None, memory_mb: None, kill_on_close: false }
    }
}

#[derive(Debug, Clone)]
pub struct LaunchedProcess {
    pub pid: u32,
    pub used_job: bool,
    pub exit_code: std::sync::Arc<std::sync::atomic::AtomicI32>,
}

impl LaunchedProcess {
    pub fn exit_code(&self) -> Option<i32> {
        match self.exit_code.load(std::sync::atomic::Ordering::Relaxed) {
            i32::MIN => None,
            code => Some(code),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriorityLevel {
    Idle,
    BelowNormal,
    Normal,
    AboveNormal,
    High,
}

impl PriorityLevel {
    pub fn win32_value(&self) -> u32 {
        match self {
            PriorityLevel::Idle => 0x40,
            PriorityLevel::BelowNormal => 0x4000,
            PriorityLevel::Normal => 0x20,
            PriorityLevel::AboveNormal => 0x8000,
            PriorityLevel::High => 0x80,
        }
    }
}

impl From<rofarm_core::lane::PriorityClass> for PriorityLevel {
    fn from(p: rofarm_core::lane::PriorityClass) -> Self {
        use rofarm_core::lane::PriorityClass;
        match p {
            PriorityClass::Idle => PriorityLevel::Idle,
            PriorityClass::BelowNormal => PriorityLevel::BelowNormal,
            PriorityClass::Normal => PriorityLevel::Normal,
            PriorityClass::AboveNormal => PriorityLevel::AboveNormal,
            PriorityClass::High => PriorityLevel::High,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatteryStatus {
    pub percent: Option<u8>,
    pub on_battery: bool,
    pub charging: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutostartStatus {
    pub enabled: bool,
    pub task_name: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardGuard {
    pub clear_after_seconds: u32,
    pub expires_at: DateTime<Utc>,
}
