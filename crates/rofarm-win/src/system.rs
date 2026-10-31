use crate::error::PlatformResult;
use crate::model::{BatteryStatus, SystemMetrics};
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct SystemSampler {
    last: std::sync::Mutex<Option<(std::time::Instant, u64, u64)>>,
}

impl SystemSampler {
    pub fn cpu_percent(&self) -> f32 {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::FILETIME;
            use windows_sys::Win32::System::Threading::GetSystemTimes;
            let mut idle = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
            let mut kernel = idle;
            let mut user = idle;
            let ok = unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) };
            if ok == 0 {
                return 0.0;
            }
            let to_u64 = |ft: FILETIME| ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;
            let idle_t = to_u64(idle);
            let kernel_t = to_u64(kernel);
            let user_t = to_u64(user);
            let total = kernel_t.saturating_add(user_t);
            let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
            let pct = if let Some((at, last_idle, last_total)) = *last {
                let elapsed = at.elapsed().as_secs_f64();
                if elapsed > 0.05 && total > last_total {
                    let total_delta = (total - last_total) as f64;
                    let idle_delta = idle_t.saturating_sub(last_idle) as f64;
                    (((total_delta - idle_delta) / total_delta) * 100.0).clamp(0.0, 100.0) as f32
                } else {
                    0.0
                }
            } else {
                0.0
            };
            *last = Some((std::time::Instant::now(), idle_t, total));
            return pct;
        }
        #[cfg(not(windows))]
        {
            0.0
        }
    }
}

pub fn system_metrics(sampler: &SystemSampler) -> PlatformResult<SystemMetrics> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut mem: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        mem.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        let memo_ok = unsafe { GlobalMemoryStatusEx(&mut mem) };
        let battery = battery_status();
        let logical = std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1);
        Ok(SystemMetrics {
            cpu_pct: sampler.cpu_percent(),
            mem_total_mb: if memo_ok != 0 { (mem.ullTotalPhys / (1024 * 1024)) as u32 } else { 0 },
            mem_used_mb: if memo_ok != 0 {
                ((mem.ullTotalPhys - mem.ullAvailPhys) / (1024 * 1024)) as u32
            } else {
                0
            },
            gpu_pct: None,
            net_up_kbps: None,
            net_down_kbps: None,
            battery_percent: battery.percent,
            on_battery: battery.on_battery,
            session_locked: session_locked(),
            logical_cores: logical,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = sampler;
        Ok(SystemMetrics::default())
    }
}

pub fn battery_status() -> BatteryStatus {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
        let mut status: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
        let ok = unsafe { GetSystemPowerStatus(&mut status) };
        if ok == 0 {
            return BatteryStatus { percent: None, on_battery: false, charging: false };
        }
        BatteryStatus {
            percent: if status.BatteryLifePercent == 255 { None } else { Some(status.BatteryLifePercent) },
            on_battery: status.ACLineStatus == 0,
            charging: status.BatteryFlag & 8 != 0,
        }
    }
    #[cfg(not(windows))]
    {
        BatteryStatus { percent: None, on_battery: false, charging: false }
    }
}

pub fn session_locked() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::StationsAndDesktops::{CloseDesktop, OpenInputDesktop};
        const DESKTOP_SWITCHDESKTOP: u32 = 0x0100;
        let desktop = unsafe { OpenInputDesktop(0, 0, DESKTOP_SWITCHDESKTOP) };
        if desktop.is_null() {
            return true;
        }
        unsafe {
            CloseDesktop(desktop);
        }
        false
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub fn work_area() -> rofarm_core::window::Rect {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWORKAREA};
        let mut rect: windows_sys::Win32::Foundation::RECT = unsafe { std::mem::zeroed() };
        let ok = unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut rect as *mut _ as *mut _, 0) };
        if ok != 0 {
            return rofarm_core::window::Rect::new(
                rect.left,
                rect.top,
                (rect.right - rect.left).max(1),
                (rect.bottom - rect.top).max(1),
            );
        }
    }
    rofarm_core::window::Rect::new(0, 0, 1920, 1080)
}

pub fn copy_to_clipboard(text: &str) -> PlatformResult<()> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::DataExchange::{
            CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
        };
        use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
        const CF_UNICODETEXT: u32 = 13;
        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return Err(crate::error::PlatformError::Win32("OpenClipboard"));
            }
            let _ = EmptyClipboard();
            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes = wide.len() * 2;
            let handle = GlobalAlloc(GMEM_MOVEABLE, bytes);
            if handle.is_null() {
                CloseClipboard();
                return Err(crate::error::PlatformError::Win32("GlobalAlloc"));
            }
            let ptr = GlobalLock(handle) as *mut u16;
            if ptr.is_null() {
                CloseClipboard();
                return Err(crate::error::PlatformError::Win32("GlobalLock"));
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
            GlobalUnlock(handle);
            SetClipboardData(CF_UNICODETEXT, handle);
            for name in ["CanIncludeInClipboardHistory", "ExcludeClipboardContentFromMonitorProcessing"] {
                let mut fmt_name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
                let fmt = RegisterClipboardFormatW(fmt_name.as_mut_ptr());
                if fmt != 0 {
                    let flag = GlobalAlloc(GMEM_MOVEABLE, 4);
                    if !flag.is_null() {
                        let p = GlobalLock(flag) as *mut u32;
                        if !p.is_null() {
                            *p = 0;
                            GlobalUnlock(flag);
                            SetClipboardData(fmt, flag);
                        }
                    }
                }
            }
            CloseClipboard();
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = text;
        Err(crate::error::PlatformError::Unsupported)
    }
}

pub fn clear_clipboard_if(expected: &str) -> PlatformResult<bool> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
        use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};
        const CF_UNICODETEXT: u32 = 13;
        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return Ok(false);
            }
            let handle = GetClipboardData(CF_UNICODETEXT);
            if handle.is_null() {
                CloseClipboard();
                return Ok(false);
            }
            let ptr = GlobalLock(handle) as *const u16;
            if ptr.is_null() {
                CloseClipboard();
                return Ok(false);
            }
            let mut len = 0usize;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            let current = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
            GlobalUnlock(handle);
            CloseClipboard();
            if current == expected {
                use windows_sys::Win32::System::DataExchange::EmptyClipboard;
                if OpenClipboard(std::ptr::null_mut()) != 0 {
                    EmptyClipboard();
                    CloseClipboard();
                }
                return Ok(true);
            }
            Ok(false)
        }
    }
    #[cfg(not(windows))]
    {
        let _ = expected;
        Err(crate::error::PlatformError::Unsupported)
    }
}

pub const AUTOSTART_TASK: &str = "RoFarm";

pub fn autostart_arguments(exe: &Path) -> Vec<String> {
    vec![
        "/Create".into(),
        "/TN".into(),
        AUTOSTART_TASK.into(),
        "/SC".into(),
        "ONLOGON".into(),
        "/RL".into(),
        "LIMITED".into(),
        "/F".into(),
        "/TR".into(),
        format!("\"{}\" --background", exe.display()),
    ]
}

pub fn autostart_command(exe: &Path) -> (String, Vec<String>) {
    ("schtasks.exe".into(), autostart_arguments(exe))
}

pub fn enable_autostart(exe: &Path) -> PlatformResult<()> {
    #[cfg(windows)]
    {
        let (program, args) = autostart_command(exe);
        let mut cmd = std::process::Command::new(program);
        cmd.args(args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let status = cmd.status()?;
        if !status.success() {
            return Err(crate::error::PlatformError::Win32("schtasks /Create"));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = exe;
        Err(crate::error::PlatformError::Unsupported)
    }
}

pub fn disable_autostart() -> PlatformResult<()> {
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("schtasks.exe");
        cmd.args(["/Delete", "/TN", AUTOSTART_TASK, "/F"]);
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
        let _ = cmd.status()?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err(crate::error::PlatformError::Unsupported)
    }
}

pub fn app_data_dir() -> std::io::Result<PathBuf> {
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        return Ok(PathBuf::from(local).join("RoFarm"));
    }
    Ok(std::env::current_dir()?.join("RoFarmData"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autostart_task_is_per_user_and_quoted() {
        let (program, args) = autostart_command(Path::new("C:\\Program Files\\RoFarm\\RoFarm.exe"));
        assert_eq!(program, "schtasks.exe");
        assert!(args.contains(&"/SC".to_string()));
        assert!(args.contains(&"ONLOGON".to_string()));
        assert!(args.iter().any(|a| a.starts_with("\"C:\\Program Files\\RoFarm\\RoFarm.exe\"")));
        assert!(!args.iter().any(|a| a.contains("/RU")));
    }

    #[test]
    fn battery_and_session_are_callable() {
        let battery = battery_status();
        assert!(battery.percent.map_or(true, |p| p <= 100));
        let _ = session_locked();
        let _ = system_metrics(&SystemSampler::default()).unwrap();
    }

    #[test]
    fn clipboard_copy_round_trip() {
        #[cfg(windows)]
        {
            let marker = format!("rofarm-test-{}", uuid_like());
            copy_to_clipboard(&marker).unwrap();
            assert!(clear_clipboard_if(&marker).unwrap());
            assert!(!clear_clipboard_if("not present").unwrap());
        }
    }

    fn uuid_like() -> u64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64
    }
}
