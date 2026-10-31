use crate::error::{PlatformError, PlatformResult};
use crate::job::{Job, JobLimits};
use crate::model::*;
use crate::process;
use crate::system;
use crate::window;
use parking_lot::Mutex;
use rofarm_core::lane::{PriorityClass, ResourceLimits};
use rofarm_core::window::Rect;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

pub trait Platform: Send + Sync {
    fn name(&self) -> &'static str;
    fn list_roblox_processes(&self) -> PlatformResult<Vec<ProcessInfo>>;
    fn is_running(&self, pid: u32) -> bool;
    fn launch_client(
        &self,
        exe: &Path,
        args: &[String],
        cwd: Option<&Path>,
        limits: Option<&ResourceLimits>,
    ) -> PlatformResult<LaunchedProcess>;
    fn open_uri(&self, uri: &str) -> PlatformResult<()>;
    fn launch_uri(&self, uri: &str) -> PlatformResult<()>;
    fn watch_process(&self, pid: u32) -> std::sync::Arc<std::sync::atomic::AtomicI32>;
    fn work_area(&self) -> Rect;
    fn terminate_process(&self, pid: u32) -> PlatformResult<()>;
    fn process_metrics(&self, pids: &[u32]) -> Vec<ProcessMetrics>;
    fn apply_limits(&self, pid: u32, limits: &ResourceLimits) -> PlatformResult<()>;
    fn trim_working_set(&self, pid: u32) -> PlatformResult<()>;
    fn list_windows(&self, pid: Option<u32>) -> PlatformResult<Vec<WindowInfo>>;
    fn move_window(&self, id: WindowId, rect: Rect, always_on_top: bool, borderless: bool) -> PlatformResult<()>;
    fn focus_window(&self, id: WindowId) -> PlatformResult<()>;
    fn set_capture_exclusion(&self, id: WindowId, excluded: bool) -> PlatformResult<()>;
    fn register_thumbnail(&self, destination: WindowId, source: WindowId) -> PlatformResult<u64>;
    fn update_thumbnail(&self, destination: WindowId, source: WindowId, rect: Rect, visible: bool) -> PlatformResult<()>;
    fn unregister_thumbnail(&self, destination: WindowId, source: WindowId) -> PlatformResult<()>;
    fn system_metrics(&self) -> SystemMetrics;
    fn battery(&self) -> BatteryStatus;
    fn session_locked(&self) -> bool;
    fn anti_afk(&self, hwnd: WindowId, foreground: bool) -> PlatformResult<()>;
    fn copy_to_clipboard(&self, text: &str) -> PlatformResult<()>;
    fn clear_clipboard(&self, expected: &str) -> bool;
    fn set_autostart(&self, enabled: bool, exe: &Path) -> PlatformResult<AutostartStatus>;
    fn release_singleton_mutex(&self) -> bool;
}

pub struct WindowsPlatform {
    sampler: process::CpuSampler,
    system_sampler: system::SystemSampler,
    thumbnails: window::DwmThumbnails,
    jobs: Mutex<HashMap<u32, Arc<Job>>>,
}

impl std::fmt::Debug for WindowsPlatform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WindowsPlatform")
    }
}

impl Default for WindowsPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsPlatform {
    pub fn new() -> Self {
        Self {
            sampler: process::CpuSampler::default(),
            system_sampler: system::SystemSampler::default(),
            thumbnails: window::DwmThumbnails::default(),
            jobs: Mutex::new(HashMap::new()),
        }
    }

    pub fn job_handle(&self, pid: u32) -> Option<Arc<Job>> {
        self.jobs.lock().get(&pid).cloned()
    }
}

fn job_limits(limits: &ResourceLimits) -> JobLimits {
    JobLimits { cpu_percent: limits.cpu_percent, memory_mb: limits.memory_mb, kill_on_close: false }
}

fn priority_of(p: PriorityClass) -> PriorityLevel {
    p.into()
}

impl Platform for WindowsPlatform {
    fn name(&self) -> &'static str {
        "windows"
    }

    fn list_roblox_processes(&self) -> PlatformResult<Vec<ProcessInfo>> {
        process::list_processes(true)
    }

    fn is_running(&self, pid: u32) -> bool {
        process::is_running(pid)
    }

    fn launch_client(
        &self,
        exe: &Path,
        args: &[String],
        cwd: Option<&Path>,
        limits: Option<&ResourceLimits>,
    ) -> PlatformResult<LaunchedProcess> {
        let job = if let Some(limits) = limits {
            let job = Job::create(&format!("RoFarm/{}", exe.file_name().unwrap_or_default().to_string_lossy()))?;
            job.set_limits(job_limits(limits))?;
            Some(job)
        } else {
            None
        };
        #[cfg(windows)]
        let result = process::launch_suspended(exe, args, cwd, job.as_ref())?;
        #[cfg(not(windows))]
        let result = process::launch_suspended(exe, args, cwd, job.as_ref())?;
        if let Some(job) = job {
            self.jobs.lock().insert(result.pid, Arc::new(job));
        }
        if let Some(limits) = limits {
            let _ = process::set_priority(result.pid, priority_of(limits.priority));
            if limits.eco_qos {
                let _ = process::set_eco_qos(result.pid, true);
            }
            if let Some(mask) = limits.affinity_mask {
                let _ = process::set_affinity(result.pid, mask);
            }
            if limits.trim_memory {
                let _ = process::trim_working_set(result.pid);
            }
            let _ = process::set_io_priority(result.pid, matches!(limits.io_priority, rofarm_core::lane::IoPriority::Low | rofarm_core::lane::IoPriority::VeryLow));
        }
        Ok(result)
    }

    fn open_uri(&self, uri: &str) -> PlatformResult<()> {
        process::open_uri(uri)
    }

    fn launch_uri(&self, uri: &str) -> PlatformResult<()> {
        process::open_uri(uri)
    }

    fn watch_process(&self, pid: u32) -> std::sync::Arc<std::sync::atomic::AtomicI32> {
        process::watch_exit(pid)
    }

    fn work_area(&self) -> Rect {
        system::work_area()
    }

    fn terminate_process(&self, pid: u32) -> PlatformResult<()> {
        self.jobs.lock().remove(&pid);
        process::terminate(pid, 3000)
    }

    fn process_metrics(&self, pids: &[u32]) -> Vec<ProcessMetrics> {
        pids.iter().map(|pid| process::metrics(*pid, &self.sampler)).collect()
    }

    fn apply_limits(&self, pid: u32, limits: &ResourceLimits) -> PlatformResult<()> {
        let job = match self.jobs.lock().get(&pid) {
            Some(job) => job.clone(),
            None => {
                let job = Arc::new(Job::create(&format!("RoFarm/live/{pid}"))?);
                job.assign_pid(pid)?;
                self.jobs.lock().insert(pid, job.clone());
                job
            }
        };
        job.set_limits(job_limits(limits))?;
        process::set_priority(pid, priority_of(limits.priority))?;
        if limits.eco_qos {
            process::set_eco_qos(pid, true)?;
        }
        let _ = process::set_io_priority(pid, matches!(limits.io_priority, rofarm_core::lane::IoPriority::Low | rofarm_core::lane::IoPriority::VeryLow));
        if let Some(mask) = limits.affinity_mask {
            process::set_affinity(pid, mask)?;
        }
        if limits.trim_memory {
            let _ = process::trim_working_set(pid);
        }
        Ok(())
    }

    fn trim_working_set(&self, pid: u32) -> PlatformResult<()> {
        process::trim_working_set(pid)
    }

    fn list_windows(&self, pid: Option<u32>) -> PlatformResult<Vec<WindowInfo>> {
        window::list_windows(pid)
    }

    fn move_window(&self, id: WindowId, rect: Rect, always_on_top: bool, borderless: bool) -> PlatformResult<()> {
        window::move_window(id, rect, always_on_top, borderless)
    }

    fn focus_window(&self, id: WindowId) -> PlatformResult<()> {
        window::focus_window(id)
    }

    fn set_capture_exclusion(&self, id: WindowId, excluded: bool) -> PlatformResult<()> {
        window::set_capture_exclusion(id, excluded)
    }

    fn register_thumbnail(&self, destination: WindowId, source: WindowId) -> PlatformResult<u64> {
        self.thumbnails.register(destination, source)
    }

    fn update_thumbnail(&self, destination: WindowId, source: WindowId, rect: Rect, visible: bool) -> PlatformResult<()> {
        self.thumbnails.update(destination, source, rect, visible)
    }

    fn unregister_thumbnail(&self, destination: WindowId, source: WindowId) -> PlatformResult<()> {
        self.thumbnails.unregister(destination, source)
    }

    fn system_metrics(&self) -> SystemMetrics {
        system::system_metrics(&self.system_sampler).unwrap_or_default()
    }

    fn battery(&self) -> BatteryStatus {
        system::battery_status()
    }

    fn session_locked(&self) -> bool {
        system::session_locked()
    }

    fn anti_afk(&self, hwnd: WindowId, foreground: bool) -> PlatformResult<()> {
        crate::input::send_anti_afk(hwnd, foreground)
    }

    fn copy_to_clipboard(&self, text: &str) -> PlatformResult<()> {
        system::copy_to_clipboard(text)
    }

    fn clear_clipboard(&self, expected: &str) -> bool {
        system::clear_clipboard_if(expected).unwrap_or(false)
    }

    fn set_autostart(&self, enabled: bool, exe: &Path) -> PlatformResult<AutostartStatus> {
        if enabled {
            system::enable_autostart(exe)?;
        } else {
            system::disable_autostart()?;
        }
        Ok(AutostartStatus {
            enabled,
            task_name: system::AUTOSTART_TASK.into(),
            detail: "Task Scheduler logon task (per user)".into(),
        })
    }

    fn release_singleton_mutex(&self) -> bool {
        process::release_singleton_mutex()
    }
}

#[derive(Default)]
pub struct MockPlatform {
    pub processes: Mutex<Vec<ProcessInfo>>,
    pub windows: Mutex<Vec<WindowInfo>>,
    pub launched: Mutex<Vec<LaunchedProcess>>,
    pub terminated: Mutex<Vec<u32>>,
    pub metrics: Mutex<Vec<ProcessMetrics>>,
    pub locked: Mutex<bool>,
    pub clipboard: Mutex<String>,
    pub jobs: Mutex<HashMap<u32, JobLimits>>,
}

impl std::fmt::Debug for MockPlatform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MockPlatform")
    }
}

impl MockPlatform {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Platform for MockPlatform {
    fn name(&self) -> &'static str {
        "mock"
    }
    fn list_roblox_processes(&self) -> PlatformResult<Vec<ProcessInfo>> {
        Ok(self.processes.lock().clone())
    }
    fn is_running(&self, pid: u32) -> bool {
        self.processes.lock().iter().any(|p| p.pid == pid)
    }
    fn launch_client(
        &self,
        _exe: &Path,
        _args: &[String],
        _cwd: Option<&Path>,
        limits: Option<&ResourceLimits>,
    ) -> PlatformResult<LaunchedProcess> {
        let pid = 10_000 + self.launched.lock().len() as u32;
        let launched = LaunchedProcess {
            pid,
            used_job: limits.is_some(),
            exit_code: std::sync::Arc::new(std::sync::atomic::AtomicI32::new(i32::MIN)),
        };
        self.launched.lock().push(launched.clone());
        self.processes.lock().push(ProcessInfo {
            pid,
            parent_pid: std::process::id(),
            name: "RobloxPlayerBeta.exe".into(),
            executable: None,
            started_at: Some(chrono::Utc::now()),
        });
        if let Some(limits) = limits {
            self.jobs.lock().insert(pid, job_limits(limits));
        }
        Ok(launched)
    }
    fn open_uri(&self, _uri: &str) -> PlatformResult<()> {
        Ok(())
    }
    fn launch_uri(&self, _uri: &str) -> PlatformResult<()> {
        let pid = 20_000 + self.launched.lock().len() as u32;
        self.launched.lock().push(LaunchedProcess {
            pid,
            used_job: false,
            exit_code: std::sync::Arc::new(std::sync::atomic::AtomicI32::new(i32::MIN)),
        });
        self.processes.lock().push(ProcessInfo {
            pid,
            parent_pid: std::process::id(),
            name: "RobloxPlayerBeta.exe".into(),
            executable: None,
            started_at: Some(chrono::Utc::now()),
        });
        Ok(())
    }
    fn watch_process(&self, _pid: u32) -> std::sync::Arc<std::sync::atomic::AtomicI32> {
        std::sync::Arc::new(std::sync::atomic::AtomicI32::new(i32::MIN))
    }
    fn work_area(&self) -> Rect {
        Rect::new(0, 0, 1920, 1080)
    }
    fn terminate_process(&self, pid: u32) -> PlatformResult<()> {
        self.terminated.lock().push(pid);
        self.processes.lock().retain(|p| p.pid != pid);
        Ok(())
    }
    fn process_metrics(&self, pids: &[u32]) -> Vec<ProcessMetrics> {
        let metrics = self.metrics.lock();
        pids.iter()
            .map(|pid| {
                metrics
                    .iter()
                    .find(|m| m.pid == *pid)
                    .copied()
                    .unwrap_or(ProcessMetrics { pid: *pid, exited: true, ..Default::default() })
            })
            .collect()
    }
    fn apply_limits(&self, pid: u32, limits: &ResourceLimits) -> PlatformResult<()> {
        self.jobs.lock().insert(pid, job_limits(limits));
        Ok(())
    }
    fn trim_working_set(&self, _pid: u32) -> PlatformResult<()> {
        Ok(())
    }
    fn list_windows(&self, pid: Option<u32>) -> PlatformResult<Vec<WindowInfo>> {
        Ok(self.windows.lock().iter().filter(|w| pid.map_or(true, |p| w.pid == p)).cloned().collect())
    }
    fn move_window(&self, _id: WindowId, _rect: Rect, _always_on_top: bool, _borderless: bool) -> PlatformResult<()> {
        Ok(())
    }
    fn focus_window(&self, _id: WindowId) -> PlatformResult<()> {
        Ok(())
    }
    fn set_capture_exclusion(&self, _id: WindowId, _excluded: bool) -> PlatformResult<()> {
        Ok(())
    }
    fn register_thumbnail(&self, _d: WindowId, _s: WindowId) -> PlatformResult<u64> {
        Ok(1)
    }
    fn update_thumbnail(&self, _d: WindowId, _s: WindowId, _r: Rect, _v: bool) -> PlatformResult<()> {
        Ok(())
    }
    fn unregister_thumbnail(&self, _d: WindowId, _s: WindowId) -> PlatformResult<()> {
        Ok(())
    }
    fn system_metrics(&self) -> SystemMetrics {
        SystemMetrics {
            cpu_pct: 12.5,
            mem_total_mb: 32_768,
            mem_used_mb: 9_216,
            logical_cores: 16,
            session_locked: *self.locked.lock(),
            ..Default::default()
        }
    }
    fn battery(&self) -> BatteryStatus {
        BatteryStatus { percent: Some(88), on_battery: false, charging: false }
    }
    fn session_locked(&self) -> bool {
        *self.locked.lock()
    }
    fn anti_afk(&self, _hwnd: WindowId, _foreground: bool) -> PlatformResult<()> {
        Ok(())
    }
    fn copy_to_clipboard(&self, text: &str) -> PlatformResult<()> {
        *self.clipboard.lock() = text.to_string();
        Ok(())
    }
    fn clear_clipboard(&self, expected: &str) -> bool {
        let mut c = self.clipboard.lock();
        if c.as_str() == expected {
            c.clear();
            true
        } else {
            false
        }
    }
    fn set_autostart(&self, enabled: bool, _exe: &Path) -> PlatformResult<AutostartStatus> {
        Ok(AutostartStatus { enabled, task_name: system::AUTOSTART_TASK.into(), detail: "mock".into() })
    }
    fn release_singleton_mutex(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_platform_lifecycle() {
        let platform = MockPlatform::new();
        assert!(platform.list_roblox_processes().unwrap().is_empty());
        let limits = ResourceLimits::default();
        let launched = platform.launch_client(Path::new("RobloxPlayerBeta.exe"), &[], None, Some(&limits)).unwrap();
        assert!(launched.used_job);
        assert!(platform.is_running(launched.pid));
        assert_eq!(platform.list_roblox_processes().unwrap().len(), 1);
        platform.apply_limits(launched.pid, &limits).unwrap();
        assert!(platform.jobs.lock().contains_key(&launched.pid));
        platform.terminate_process(launched.pid).unwrap();
        assert!(!platform.is_running(launched.pid));
        assert_eq!(platform.terminated.lock().len(), 1);
    }

    #[test]
    fn clipboard_guard_only_clears_matching_content() {
        let platform = MockPlatform::new();
        platform.copy_to_clipboard("secret").unwrap();
        assert!(!platform.clear_clipboard("other"));
        assert!(platform.clear_clipboard("secret"));
        assert!(!platform.clear_clipboard("secret"));
    }

    #[test]
    fn session_lock_and_metrics_are_reportable() {
        let platform = MockPlatform::new();
        assert!(!platform.session_locked());
        *platform.locked.lock() = true;
        assert!(platform.session_locked());
        assert_eq!(platform.system_metrics().logical_cores, 16);
        assert_eq!(platform.battery().percent, Some(88));
    }

    #[cfg(windows)]
    #[test]
    fn windows_platform_smoke() {
        let platform = WindowsPlatform::new();
        assert_eq!(platform.name(), "windows");
        let metrics = platform.system_metrics();
        assert!(metrics.mem_total_mb > 0);
        let _ = platform.list_roblox_processes().unwrap();
        let _ = platform.battery();
    }
}
