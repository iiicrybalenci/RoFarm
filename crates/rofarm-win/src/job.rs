use crate::error::{PlatformError, PlatformResult};
pub use crate::model::JobLimits;
use std::sync::Arc;

#[cfg(windows)]
mod imp {
    use super::*;
    use std::mem::{size_of, zeroed};
    use std::sync::Arc;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectCpuRateControlInformation,
        JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
        JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_CPU_RATE_CONTROL_INFORMATION,
        JOB_OBJECT_CPU_RATE_CONTROL_ENABLE, JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_JOB_MEMORY, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE};

    pub struct JobHandle {
        handle: HANDLE,
        pub name: String,
    }

    unsafe impl Send for JobHandle {}
    unsafe impl Sync for JobHandle {}

    impl Drop for JobHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }

    #[derive(Clone)]
    pub struct Job {
        inner: Arc<JobHandle>,
    }

    impl std::fmt::Debug for Job {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Job").field("name", &self.inner.name).finish()
        }
    }

    fn wide(name: &str) -> Vec<u16> {
        name.encode_utf16().chain(std::iter::once(0)).collect()
    }

    impl Job {
        pub fn create(name: &str) -> PlatformResult<Self> {
            let wname = wide(name);
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), wname.as_ptr()) };
            if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                return Err(PlatformError::Win32("CreateJobObjectW"));
            }
            Ok(Self { inner: Arc::new(JobHandle { handle, name: name.into() }) })
        }

        pub fn assign_pid(&self, pid: u32) -> PlatformResult<()> {
            let process = unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid) };
            if process.is_null() {
                return Err(PlatformError::NotFound);
            }
            let ok = unsafe { AssignProcessToJobObject(self.inner.handle, process) };
            unsafe {
                CloseHandle(process);
            }
            if ok == 0 {
                return Err(PlatformError::Win32("AssignProcessToJobObject"));
            }
            Ok(())
        }

        pub fn set_limits(&self, limits: JobLimits) -> PlatformResult<()> {
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
            if limits.kill_on_close {
                info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            }
            if let Some(mem) = limits.memory_mb {
                let bytes = (mem as usize).saturating_mul(1024 * 1024);
                info.ProcessMemoryLimit = bytes;
                info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_PROCESS_MEMORY;
            }
            let ok = unsafe {
                SetInformationJobObject(
                    self.inner.handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const core::ffi::c_void,
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };
            if ok == 0 {
                return Err(PlatformError::Win32("SetInformationJobObject"));
            }
            if let Some(cpu) = limits.cpu_percent {
                let cpu = cpu.clamp(1, 100) as u32;
                let mut control: JOBOBJECT_CPU_RATE_CONTROL_INFORMATION = unsafe { zeroed() };
                control.ControlFlags = JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP;
                control.Anonymous.CpuRate = cpu * 100;
                let ok = unsafe {
                    SetInformationJobObject(
                        self.inner.handle,
                        JobObjectCpuRateControlInformation,
                        &control as *const _ as *const core::ffi::c_void,
                        size_of::<JOBOBJECT_CPU_RATE_CONTROL_INFORMATION>() as u32,
                    )
                };
                if ok == 0 {
                    return Err(PlatformError::Win32("SetInformationJobObject(cpu)"));
                }
            }
            Ok(())
        }

        pub fn terminate(&self) -> PlatformResult<()> {
            let ok = unsafe { TerminateJobObject(self.inner.handle, 1) };
            if ok == 0 {
                return Err(PlatformError::Win32("TerminateJobObject"));
            }
            Ok(())
        }

        pub fn cpu_time_100ns(&self) -> Option<u64> {
            let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
            let mut returned = 0u32;
            let ok = unsafe {
                QueryInformationJobObject(
                    self.inner.handle,
                    windows_sys::Win32::System::JobObjects::JobObjectBasicAccountingInformation,
                    &mut info as *mut _ as *mut core::ffi::c_void,
                    size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                    &mut returned,
                )
            };
            if ok == 0 {
                return None;
            }
            let total = (info.TotalUserTime as u64) + (info.TotalKernelTime as u64);
            Some(total)
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    #[derive(Clone)]
    pub struct Job;

    impl std::fmt::Debug for Job {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("Job(unsupported)")
        }
    }

    impl Job {
        pub fn create(_name: &str) -> PlatformResult<Self> {
            Ok(Self)
        }
        pub fn assign_pid(&self, _pid: u32) -> PlatformResult<()> {
            Err(PlatformError::Unsupported)
        }
        pub fn set_limits(&self, _limits: JobLimits) -> PlatformResult<()> {
            Err(PlatformError::Unsupported)
        }
        pub fn terminate(&self) -> PlatformResult<()> {
            Err(PlatformError::Unsupported)
        }
        pub fn cpu_time_100ns(&self) -> Option<u64> {
            None
        }
    }
}

pub use imp::Job;

#[derive(Debug, Clone)]
pub struct JobLimiter {
    pub job: Arc<Job>,
}

pub fn apply_job_limits(pid: u32, limits: JobLimits) -> PlatformResult<Arc<Job>> {
    let job = Job::create(&format!("RoFarm/client/{pid}"))?;
    job.set_limits(limits)?;
    job.assign_pid(pid)?;
    Ok(Arc::new(job))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_validate_through_job_api() {
        #[cfg(windows)]
        {
            let job = Job::create("RoFarm/test").unwrap();
            job.set_limits(JobLimits { cpu_percent: Some(25), memory_mb: Some(1024), kill_on_close: false }).unwrap();
            assert!(job.cpu_time_100ns().is_some());
        }
        #[cfg(not(windows))]
        {
            assert!(matches!(Job::create("x"), Ok(_)));
        }
    }
}
