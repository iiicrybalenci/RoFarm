use crate::error::{PlatformError, PlatformResult};
use crate::model::{LaunchedProcess, PriorityLevel, ProcessInfo, ProcessMetrics};
use std::path::PathBuf;

pub fn quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.chars().any(|c| c == ' ' || c == '\t' || c == '"' || c == '\n') {
        return arg.to_string();
    }
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(backslashes));
                backslashes = 0;
                out.push(c);
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out.push('"');
    out
}

pub fn build_command_line(exe: &str, args: &[String]) -> String {
    let mut line = quote_arg(exe);
    for arg in args {
        line.push(' ');
        line.push_str(&quote_arg(arg));
    }
    line
}

#[cfg(windows)]
mod imp {
    use super::*;
    use crate::job::Job;
    use chrono::{DateTime, TimeZone, Utc};
    use parking_lot::Mutex;
    use std::collections::HashMap;
    use std::mem::{size_of, zeroed};
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Instant;
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, FILETIME, HANDLE, INVALID_HANDLE_VALUE, STILL_ACTIVE,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, Thread32First, Thread32Next,
        PROCESSENTRY32W, TH32CS_SNAPPROCESS, TH32CS_SNAPTHREAD, THREADENTRY32,
    };
    use windows_sys::Win32::System::ProcessStatus::{K32EmptyWorkingSet, K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, GetExitCodeProcess, GetPriorityClass, GetProcessHandleCount, GetProcessTimes,
        OpenProcess, QueryFullProcessImageNameW, ResumeThread, SetPriorityClass, SetProcessAffinityMask,
        SetProcessInformation, TerminateProcess, WaitForSingleObject, CREATE_NEW_PROCESS_GROUP,
        CREATE_SUSPENDED, INFINITE, PROCESS_INFORMATION_CLASS, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        PROCESS_POWER_THROTTLING_STATE, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION,
        PROCESS_SET_QUOTA, PROCESS_TERMINATE, PROCESS_VM_READ, STARTUPINFOW,
    };

    const PROCESS_IO_PRIORITY_CLASS: PROCESS_INFORMATION_CLASS = 3;
    const PROCESS_POWER_THROTTLING_CURRENT_VERSION: u32 = 1;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn utf16_string(buf: &[u16]) -> String {
        let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
    }

    fn process_path(pid: u32) -> Option<PathBuf> {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size) };
        unsafe {
            CloseHandle(handle);
        }
        if ok == 0 {
            None
        } else {
            Some(PathBuf::from(utf16_string(&buf[..size as usize])))
        }
    }

    fn filetime_to_utc(ft: FILETIME) -> Option<DateTime<Utc>> {
        let ticks = ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;
        if ticks == 0 {
            return None;
        }
        let unix_secs = (ticks / 10_000_000) as i64 - 11_644_473_600;
        Utc.timestamp_opt(unix_secs, 0).single()
    }

    fn process_created(handle: HANDLE) -> Option<DateTime<Utc>> {
        let mut creation = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let mut exit = creation;
        let mut kernel = creation;
        let mut user = creation;
        let ok = unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) };
        if ok == 0 {
            None
        } else {
            filetime_to_utc(creation)
        }
    }

    pub fn list_processes(filter_roblox: bool) -> PlatformResult<Vec<ProcessInfo>> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(PlatformError::Win32("CreateToolhelp32Snapshot"));
        }
        let mut entry: PROCESSENTRY32W = unsafe { zeroed() };
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut out = Vec::new();
        let mut ok = unsafe { Process32FirstW(snapshot, &mut entry) };
        while ok != 0 {
            let name = utf16_string(&entry.szExeFile);
            if !filter_roblox || name.eq_ignore_ascii_case("RobloxPlayerBeta.exe") {
                let executable = process_path(entry.th32ProcessID);
                let started_at = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.th32ProcessID) }
                    .pipe_non_null()
                    .and_then(|h| {
                        let created = process_created(h);
                        unsafe {
                            CloseHandle(h);
                        }
                        created
                    });
                out.push(ProcessInfo {
                    pid: entry.th32ProcessID,
                    parent_pid: entry.th32ParentProcessID,
                    name,
                    executable,
                    started_at,
                });
            }
            entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
            ok = unsafe { Process32NextW(snapshot, &mut entry) };
        }
        unsafe {
            CloseHandle(snapshot);
        }
        Ok(out)
    }

    trait HandleExt {
        fn pipe_non_null(self) -> Option<HANDLE>;
    }
    impl HandleExt for HANDLE {
        fn pipe_non_null(self) -> Option<HANDLE> {
            if self.is_null() {
                None
            } else {
                Some(self)
            }
        }
    }

    pub fn is_running(pid: u32) -> bool {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return false;
        }
        let mut code: u32 = 0;
        let ok = unsafe { GetExitCodeProcess(handle, &mut code) };
        unsafe {
            CloseHandle(handle);
        }
        ok != 0 && code == STILL_ACTIVE as u32
    }

    pub fn terminate(pid: u32, wait_ms: u32) -> PlatformResult<()> {
        const SYNCHRONIZE: u32 = 0x0010_0000;
        let handle = unsafe { OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, 0, pid) };
        if handle.is_null() {
            let code = unsafe { GetLastError() };
            return Err(if code == 5 { PlatformError::AccessDenied } else { PlatformError::NotFound });
        }
        let ok = unsafe { TerminateProcess(handle, 1) };
        if ok != 0 && wait_ms > 0 {
            unsafe {
                WaitForSingleObject(handle, wait_ms);
            }
        }
        unsafe {
            CloseHandle(handle);
        }
        if ok == 0 {
            return Err(PlatformError::Win32("TerminateProcess"));
        }
        Ok(())
    }

    pub fn launch_suspended(
        exe: &Path,
        args: &[String],
        cwd: Option<&Path>,
        job: Option<&Job>,
    ) -> PlatformResult<LaunchedProcess> {
        let exe_string = exe.to_string_lossy().to_string();
        let line = build_command_line(&exe_string, args);
        let app_wide = wide(&exe_string);
        let mut cmd_wide: Vec<u16> = line.encode_utf16().chain(std::iter::once(0)).collect();
        let cwd_wide = cwd.map(|c| wide(&c.to_string_lossy()));
        let mut startup: STARTUPINFOW = unsafe { zeroed() };
        startup.cb = size_of::<STARTUPINFOW>() as u32;
        let mut info: windows_sys::Win32::System::Threading::PROCESS_INFORMATION = unsafe { zeroed() };
        let ok = unsafe {
            CreateProcessW(
                app_wide.as_ptr(),
                cmd_wide.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                CREATE_SUSPENDED | CREATE_NEW_PROCESS_GROUP,
                std::ptr::null(),
                cwd_wide.as_ref().map(|c| c.as_ptr()).unwrap_or(std::ptr::null()),
                &startup,
                &mut info,
            )
        };
        if ok == 0 {
            return Err(PlatformError::Win32("CreateProcessW"));
        }
        let mut used_job = false;
        if let Some(job) = job {
            if job.assign_pid(info.dwProcessId).is_ok() {
                used_job = true;
            } else {
                unsafe {
                    TerminateProcess(info.hProcess, 1);
                    CloseHandle(info.hThread);
                    CloseHandle(info.hProcess);
                }
                return Err(PlatformError::Win32("AssignProcessToJobObject"));
            }
        }
        let resumed = unsafe { ResumeThread(info.hThread) };
        unsafe {
            CloseHandle(info.hThread);
        }
        if resumed == u32::MAX {
            unsafe {
                TerminateProcess(info.hProcess, 1);
                CloseHandle(info.hProcess);
            }
            return Err(PlatformError::Win32("ResumeThread"));
        }
        let exit_code = std::sync::Arc::new(std::sync::atomic::AtomicI32::new(i32::MIN));
        let cell = exit_code.clone();
        let process_handle = info.hProcess as isize;
        std::thread::Builder::new()
            .name(format!("wait-{}", info.dwProcessId))
            .spawn(move || unsafe {
                const INFINITE: u32 = 0xFFFF_FFFF;
                let process_handle = process_handle as HANDLE;
                WaitForSingleObject(process_handle, INFINITE);
                let mut code: u32 = 0;
                if GetExitCodeProcess(process_handle, &mut code) != 0 {
                    cell.store(code as i32, std::sync::atomic::Ordering::Relaxed);
                }
                CloseHandle(process_handle);
            })
            .ok();
        Ok(LaunchedProcess { pid: info.dwProcessId, used_job, exit_code })
    }

    pub fn watch_exit(pid: u32) -> std::sync::Arc<std::sync::atomic::AtomicI32> {
        let cell = std::sync::Arc::new(std::sync::atomic::AtomicI32::new(i32::MIN));
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | 0x0010_0000, 0, pid) };
        if handle.is_null() {
            return cell;
        }
        let cell_for_thread = cell.clone();
        let handle = handle as isize;
        std::thread::Builder::new()
            .name(format!("watch-{pid}"))
            .spawn(move || unsafe {
                const INFINITE: u32 = 0xFFFF_FFFF;
                let handle = handle as HANDLE;
                WaitForSingleObject(handle, INFINITE);
                let mut code: u32 = 0;
                if GetExitCodeProcess(handle, &mut code) != 0 {
                    cell_for_thread.store(code as i32, std::sync::atomic::Ordering::Relaxed);
                }
                CloseHandle(handle);
            })
            .ok();
        cell
    }

    pub fn open_uri(uri: &str) -> PlatformResult<()> {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let operation = wide("open");
        let uri_wide = wide(uri);
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                uri_wide.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if (result as isize) <= 32 {
            return Err(PlatformError::Win32("ShellExecuteW"));
        }
        Ok(())
    }

    fn open(pid: u32, access: u32) -> PlatformResult<HANDLE> {
        let handle = unsafe { OpenProcess(access, 0, pid) };
        if handle.is_null() {
            Err(PlatformError::NotFound)
        } else {
            Ok(handle)
        }
    }

    pub fn set_priority(pid: u32, priority: PriorityLevel) -> PlatformResult<()> {
        let handle = open(pid, PROCESS_SET_INFORMATION)?;
        let ok = unsafe { SetPriorityClass(handle, priority.win32_value()) };
        unsafe {
            CloseHandle(handle);
        }
        if ok == 0 {
            return Err(PlatformError::Win32("SetPriorityClass"));
        }
        Ok(())
    }

    pub fn get_priority(pid: u32) -> PlatformResult<u32> {
        let handle = open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
        let value = unsafe { GetPriorityClass(handle) };
        unsafe {
            CloseHandle(handle);
        }
        if value == 0 {
            return Err(PlatformError::Win32("GetPriorityClass"));
        }
        Ok(value)
    }

    pub fn set_affinity(pid: u32, mask: u64) -> PlatformResult<()> {
        let handle = open(pid, PROCESS_SET_INFORMATION)?;
        let ok = unsafe { SetProcessAffinityMask(handle, mask as usize) };
        unsafe {
            CloseHandle(handle);
        }
        if ok == 0 {
            return Err(PlatformError::Win32("SetProcessAffinityMask"));
        }
        Ok(())
    }

    pub fn trim_working_set(pid: u32) -> PlatformResult<()> {
        let handle = open(pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA)?;
        let ok = unsafe { K32EmptyWorkingSet(handle) };
        unsafe {
            CloseHandle(handle);
        }
        if ok == 0 {
            return Err(PlatformError::Win32("K32EmptyWorkingSet"));
        }
        Ok(())
    }

    pub fn set_eco_qos(pid: u32, enabled: bool) -> PlatformResult<()> {
        let handle = open(pid, PROCESS_SET_INFORMATION)?;
        let mut state = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            StateMask: if enabled { PROCESS_POWER_THROTTLING_EXECUTION_SPEED } else { 0 },
        };
        let ok = unsafe {
            SetProcessInformation(
                handle,
                windows_sys::Win32::System::Threading::ProcessPowerThrottling,
                &mut state as *mut _ as *const core::ffi::c_void,
                size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
            )
        };
        unsafe {
            CloseHandle(handle);
        }
        if ok == 0 {
            return Err(PlatformError::Win32("SetProcessInformation(PowerThrottling)"));
        }
        Ok(())
    }

    pub fn set_io_priority(pid: u32, low: bool) -> PlatformResult<()> {
        #[repr(C)]
        struct IoPriorityHint {
            value: i32,
        }
        let handle = open(pid, PROCESS_SET_INFORMATION)?;
        let hint = IoPriorityHint { value: if low { 0 } else { 2 } };
        let ok = unsafe {
            SetProcessInformation(
                handle,
                PROCESS_IO_PRIORITY_CLASS,
                &hint as *const _ as *const core::ffi::c_void,
                size_of::<IoPriorityHint>() as u32,
            )
        };
        unsafe {
            CloseHandle(handle);
        }
        if ok == 0 {
            return Err(PlatformError::Win32("SetProcessInformation(IoPriority)"));
        }
        Ok(())
    }

    fn thread_count(pid: u32) -> u32 {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return 0;
        }
        let mut entry: THREADENTRY32 = unsafe { zeroed() };
        entry.dwSize = size_of::<THREADENTRY32>() as u32;
        let mut count = 0;
        let mut ok = unsafe { Thread32First(snapshot, &mut entry) };
        while ok != 0 {
            if entry.th32OwnerProcessID == pid {
                count += 1;
            }
            entry.dwSize = size_of::<THREADENTRY32>() as u32;
            ok = unsafe { Thread32Next(snapshot, &mut entry) };
        }
        unsafe {
            CloseHandle(snapshot);
        }
        count
    }

    #[derive(Default)]
    pub struct CpuSampler {
        last: Mutex<HashMap<u32, (Instant, u64)>>,
    }

    impl CpuSampler {
        fn percent(&self, pid: u32, cpu_100ns: u64) -> f32 {
            let now = Instant::now();
            let cores = std::thread::available_parallelism().map(|n| n.get() as f32).unwrap_or(1.0);
            let mut last = self.last.lock();
            let pct = if let Some((at, prev)) = last.get(&pid) {
                let elapsed = now.duration_since(*at).as_secs_f64();
                if elapsed < 0.05 || cpu_100ns <= *prev {
                    0.0
                } else {
                    let delta = (cpu_100ns - *prev) as f64 * 100.0 / 10_000_000.0;
                    (delta / elapsed / cores as f64 * 100.0) as f32
                }
            } else {
                0.0
            };
            last.insert(pid, (now, cpu_100ns));
            pct.clamp(0.0, 100.0)
        }
    }

    pub fn metrics(pid: u32, sampler: &CpuSampler) -> ProcessMetrics {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid) };
        if handle.is_null() {
            return ProcessMetrics { pid, exited: true, ..Default::default() };
        }
        let mut code: u32 = 0;
        let alive = unsafe { GetExitCodeProcess(handle, &mut code) } != 0 && code == STILL_ACTIVE as u32;
        if !alive {
            unsafe {
                CloseHandle(handle);
            }
            return ProcessMetrics { pid, exited: true, ..Default::default() };
        }
        let mut creation = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let mut exit = creation;
        let mut kernel = creation;
        let mut user = creation;
        let (cpu_100ns, handles) = unsafe {
            let times = GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user);
            let mut handles: u32 = 0;
            GetProcessHandleCount(handle, &mut handles);
            let total = if times != 0 {
                ((kernel.dwHighDateTime as u64) << 32)
                    | kernel.dwLowDateTime as u64
                    | ((((user.dwHighDateTime as u64) << 32) | user.dwLowDateTime as u64) << 32)
            } else {
                0
            };
            let k = ((kernel.dwHighDateTime as u64) << 32) | kernel.dwLowDateTime as u64;
            let u = ((user.dwHighDateTime as u64) << 32) | user.dwLowDateTime as u64;
            (k.saturating_add(u), handles)
        };
        let _ = cpu_100ns;
        let k = ((kernel.dwHighDateTime as u64) << 32) | kernel.dwLowDateTime as u64;
        let u = ((user.dwHighDateTime as u64) << 32) | user.dwLowDateTime as u64;
        let cpu_pct = sampler.percent(pid, k.saturating_add(u));
        let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { zeroed() };
        counters.cb = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let ok = unsafe { K32GetProcessMemoryInfo(handle, &mut counters, counters.cb) };
        let mem_mb = if ok != 0 { (counters.WorkingSetSize / (1024 * 1024)) as u32 } else { 0 };
        unsafe {
            CloseHandle(handle);
        }
        ProcessMetrics {
            pid,
            cpu_pct,
            mem_mb,
            thread_count: thread_count(pid),
            handle_count: handles,
            gpu_pct: None,
            vram_mb: None,
            disk_kbps: None,
            net_up_kbps: None,
            net_down_kbps: None,
            exited: false,
        }
    }

    pub fn release_singleton_mutex() -> bool {
        use windows_sys::Win32::System::Threading::{OpenMutexW, ReleaseMutex};
        const SYNCHRONIZE: u32 = 0x0010_0000;
        let name = wide("ROBLOX_singletonMutex");
        let handle = unsafe { OpenMutexW(SYNCHRONIZE, 0, name.as_ptr()) };
        if handle.is_null() {
            return false;
        }
        unsafe {
            let _ = ReleaseMutex(handle);
            CloseHandle(handle);
        }
        true
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn list_processes(_filter_roblox: bool) -> PlatformResult<Vec<ProcessInfo>> {
        Ok(Vec::new())
    }
    pub fn is_running(_pid: u32) -> bool {
        false
    }
    pub fn terminate(_pid: u32, _wait_ms: u32) -> PlatformResult<()> {
        Err(PlatformError::Unsupported)
    }
    pub fn launch_suspended(
        _exe: &std::path::Path,
        _args: &[String],
        _cwd: Option<&std::path::Path>,
        _job: Option<&crate::job::Job>,
    ) -> PlatformResult<LaunchedProcess> {
        Err(PlatformError::Unsupported)
    }
    pub fn open_uri(_uri: &str) -> PlatformResult<()> {
        Err(PlatformError::Unsupported)
    }
    pub fn watch_exit(_pid: u32) -> std::sync::Arc<std::sync::atomic::AtomicI32> {
        std::sync::Arc::new(std::sync::atomic::AtomicI32::new(i32::MIN))
    }
    pub fn set_priority(_pid: u32, _priority: PriorityLevel) -> PlatformResult<()> {
        Err(PlatformError::Unsupported)
    }
    pub fn get_priority(_pid: u32) -> PlatformResult<u32> {
        Err(PlatformError::Unsupported)
    }
    pub fn set_affinity(_pid: u32, _mask: u64) -> PlatformResult<()> {
        Err(PlatformError::Unsupported)
    }
    pub fn trim_working_set(_pid: u32) -> PlatformResult<()> {
        Err(PlatformError::Unsupported)
    }
    pub fn set_eco_qos(_pid: u32, _enabled: bool) -> PlatformResult<()> {
        Err(PlatformError::Unsupported)
    }
    pub fn set_io_priority(_pid: u32, _low: bool) -> PlatformResult<()> {
        Err(PlatformError::Unsupported)
    }
    #[derive(Default)]
    pub struct CpuSampler;
    pub fn metrics(pid: u32, _sampler: &CpuSampler) -> ProcessMetrics {
        ProcessMetrics { pid, exited: true, ..Default::default() }
    }
    pub fn release_singleton_mutex() -> bool {
        false
    }
}

pub use imp::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_follows_command_line_rules() {
        assert_eq!(quote_arg("simple"), "simple");
        assert_eq!(quote_arg("has space"), "\"has space\"");
        assert_eq!(quote_arg("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote_arg("C:\\path\\"), "\"C:\\path\\\\\"");
        assert_eq!(quote_arg(""), "\"\"");
        let line = build_command_line("C:\\Roblox\\RobloxPlayerBeta.exe", &["-b".into(), "-j".into(), "job id".into()]);
        assert!(line.starts_with("\"C:\\Roblox\\RobloxPlayerBeta.exe\" -b -j \"job id\""));
    }

    #[test]
    fn list_processes_includes_current_process() {
        #[cfg(windows)]
        {
            let pid = std::process::id();
            let processes = list_processes(false).unwrap();
            assert!(processes.iter().any(|p| p.pid == pid));
            assert!(is_running(pid));
        }
    }
}
