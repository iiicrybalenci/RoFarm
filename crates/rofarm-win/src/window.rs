use crate::error::PlatformResult;
use crate::model::{WindowId, WindowInfo};
use rofarm_core::window::Rect;
use std::collections::HashMap;

#[cfg(windows)]
mod imp {
    use super::*;
    use crate::error::PlatformError;
    use parking_lot::Mutex;
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
    use windows_sys::Win32::Graphics::Dwm::{
        DwmRegisterThumbnail, DwmUnregisterThumbnail, DwmUpdateThumbnailProperties, DWM_THUMBNAIL_PROPERTIES,
        DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION, DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowLongW, GetWindowRect, GetWindowTextW,
        GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow, SetWindowDisplayAffinity,
        SetWindowLongW, SetWindowPos, ShowWindow, GWL_STYLE, HWND_NOTOPMOST, HWND_TOP, HWND_TOPMOST,
        SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_RESTORE, WDA_EXCLUDEFROMCAPTURE,
        WDA_NONE, WS_CAPTION, WS_THICKFRAME, WS_VISIBLE,
    };

    struct EnumCtx {
        pid: Option<u32>,
        out: Vec<WindowInfo>,
    }

    unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = unsafe { &mut *(lparam as *mut EnumCtx) };
        let mut pid = 0u32;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
        }
        if let Some(target) = ctx.pid {
            if pid != target {
                return 1;
            }
        }
        if unsafe { IsWindowVisible(hwnd) } == 0 {
            return 1;
        }
        let mut rect: RECT = unsafe { zeroed() };
        if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
            return 1;
        }
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width < 100 || height < 80 {
            return 1;
        }
        let mut title_buf = [0u16; 512];
        let len = unsafe { GetWindowTextW(hwnd, title_buf.as_mut_ptr(), title_buf.len() as i32) }.max(0) as usize;
        let title = String::from_utf16_lossy(&title_buf[..len]);
        if title.is_empty() {
            return 1;
        }
        let mut class_buf = [0u16; 256];
        let clen = unsafe { GetClassNameW(hwnd, class_buf.as_mut_ptr(), class_buf.len() as i32) }.max(0) as usize;
        let class_name = String::from_utf16_lossy(&class_buf[..clen]);
        ctx.out.push(WindowInfo {
            id: WindowId::from_isize(hwnd as isize),
            pid,
            title,
            class_name,
            rect: Rect::new(rect.left, rect.top, width, height),
            visible: true,
        });
        1
    }

    pub fn list_windows(pid: Option<u32>) -> PlatformResult<Vec<WindowInfo>> {
        let mut ctx = EnumCtx { pid, out: Vec::new() };
        let ok = unsafe { EnumWindows(Some(enum_callback), &mut ctx as *mut EnumCtx as isize) };
        if ok == 0 {
            return Err(PlatformError::Win32("EnumWindows"));
        }
        Ok(ctx.out)
    }

    fn hwnd(id: WindowId) -> PlatformResult<HWND> {
        if id.is_null() {
            return Err(PlatformError::Invalid("null window handle".into()));
        }
        Ok(id.as_isize() as HWND)
    }

    pub fn move_window(id: WindowId, rect: Rect, always_on_top: bool, borderless: bool) -> PlatformResult<()> {
        let handle = hwnd(id)?;
        if borderless {
            unsafe {
                let style = GetWindowLongW(handle, GWL_STYLE) as u32;
                let new_style = (style & !(WS_CAPTION | WS_THICKFRAME)) | WS_VISIBLE;
                SetWindowLongW(handle, GWL_STYLE, new_style as i32);
            }
        }
        let after = if always_on_top { HWND_TOPMOST } else { HWND_TOP };
        let flags = SWP_NOACTIVATE | SWP_SHOWWINDOW | if borderless { SWP_FRAMECHANGED } else { 0 };
        let ok = unsafe {
            SetWindowPos(handle, after, rect.x, rect.y, rect.width, rect.height, flags)
        };
        if ok == 0 {
            return Err(PlatformError::Win32("SetWindowPos"));
        }
        Ok(())
    }

    pub fn set_always_on_top(id: WindowId, on_top: bool) -> PlatformResult<()> {
        let handle = hwnd(id)?;
        let after = if on_top { HWND_TOPMOST } else { HWND_NOTOPMOST };
        let ok = unsafe { SetWindowPos(handle, after, 0, 0, 0, 0, SWP_NOACTIVATE | 0x0001 | 0x0002) };
        if ok == 0 {
            return Err(PlatformError::Win32("SetWindowPos(topmost)"));
        }
        Ok(())
    }

    pub fn focus_window(id: WindowId) -> PlatformResult<()> {
        let handle = hwnd(id)?;
        unsafe {
            ShowWindow(handle, SW_RESTORE);
        }
        if unsafe { SetForegroundWindow(handle) } == 0 {
            return Err(PlatformError::Win32("SetForegroundWindow"));
        }
        Ok(())
    }

    pub fn set_capture_exclusion(id: WindowId, excluded: bool) -> PlatformResult<()> {
        let handle = hwnd(id)?;
        let affinity = if excluded { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE };
        if unsafe { SetWindowDisplayAffinity(handle, affinity) } == 0 {
            return Err(PlatformError::Win32("SetWindowDisplayAffinity"));
        }
        Ok(())
    }

    pub fn minimize(id: WindowId) -> PlatformResult<()> {
        unsafe {
            ShowWindow(hwnd(id)?, 6);
        }
        Ok(())
    }

    pub fn restore(id: WindowId) -> PlatformResult<()> {
        unsafe {
            ShowWindow(hwnd(id)?, SW_RESTORE);
        }
        Ok(())
    }

    #[derive(Default)]
    pub struct DwmThumbnails {
        map: Mutex<HashMap<(isize, isize), isize>>,
    }

    impl DwmThumbnails {
        pub fn register(&self, destination: WindowId, source: WindowId) -> PlatformResult<u64> {
            let mut thumb: isize = 0;
            let hr = unsafe {
                DwmRegisterThumbnail(destination.as_isize() as HWND, source.as_isize() as HWND, &mut thumb)
            };
            if hr < 0 {
                return Err(PlatformError::Win32("DwmRegisterThumbnail"));
            }
            self.map.lock().insert((destination.as_isize(), source.as_isize()), thumb);
            Ok(thumb as u64)
        }

        pub fn update(&self, destination: WindowId, source: WindowId, rect: Rect, visible: bool) -> PlatformResult<()> {
            let thumb = self
                .map
                .lock()
                .get(&(destination.as_isize(), source.as_isize()))
                .copied()
                .ok_or(PlatformError::NotFound)?;
            let props = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_OPACITY | DWM_TNP_SOURCECLIENTAREAONLY,
                rcDestination: RECT { left: rect.x, top: rect.y, right: rect.right(), bottom: rect.bottom() },
                rcSource: RECT { left: 0, top: 0, right: 0, bottom: 0 },
                opacity: 255,
                fVisible: if visible { 1 } else { 0 },
                fSourceClientAreaOnly: 0,
            };
            let hr = unsafe { DwmUpdateThumbnailProperties(thumb, &props) };
            if hr < 0 {
                return Err(PlatformError::Win32("DwmUpdateThumbnailProperties"));
            }
            Ok(())
        }

        pub fn unregister(&self, destination: WindowId, source: WindowId) -> PlatformResult<()> {
            if let Some(thumb) = self.map.lock().remove(&(destination.as_isize(), source.as_isize())) {
                let hr = unsafe { DwmUnregisterThumbnail(thumb) };
                if hr < 0 {
                    return Err(PlatformError::Win32("DwmUnregisterThumbnail"));
                }
            }
            Ok(())
        }

        pub fn count(&self) -> usize {
            self.map.lock().len()
        }

        pub fn size_of_props() -> usize {
            size_of::<DWM_THUMBNAIL_PROPERTIES>()
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    use crate::error::PlatformError;

    pub fn list_windows(_pid: Option<u32>) -> PlatformResult<Vec<WindowInfo>> {
        Ok(Vec::new())
    }
    pub fn move_window(_id: WindowId, _rect: Rect, _top: bool, _borderless: bool) -> PlatformResult<()> {
        Err(PlatformError::Unsupported)
    }
    pub fn set_always_on_top(_id: WindowId, _on_top: bool) -> PlatformResult<()> {
        Err(PlatformError::Unsupported)
    }
    pub fn focus_window(_id: WindowId) -> PlatformResult<()> {
        Err(PlatformError::Unsupported)
    }
    pub fn set_capture_exclusion(_id: WindowId, _excluded: bool) -> PlatformResult<()> {
        Err(PlatformError::Unsupported)
    }
    pub fn minimize(_id: WindowId) -> PlatformResult<()> {
        Err(PlatformError::Unsupported)
    }
    pub fn restore(_id: WindowId) -> PlatformResult<()> {
        Err(PlatformError::Unsupported)
    }

    #[derive(Default)]
    pub struct DwmThumbnails;
    impl DwmThumbnails {
        pub fn register(&self, _d: WindowId, _s: WindowId) -> PlatformResult<u64> {
            Err(PlatformError::Unsupported)
        }
        pub fn update(&self, _d: WindowId, _s: WindowId, _r: Rect, _v: bool) -> PlatformResult<()> {
            Err(PlatformError::Unsupported)
        }
        pub fn unregister(&self, _d: WindowId, _s: WindowId) -> PlatformResult<()> {
            Ok(())
        }
        pub fn count(&self) -> usize {
            0
        }
    }
}

pub use imp::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_window_is_rejected() {
        #[cfg(windows)]
        {
            assert!(move_window(WindowId(0), Rect::new(0, 0, 100, 100), false, false).is_err());
        }
        assert_eq!(WindowId::from_isize(42).0, 42);
    }

    #[cfg(not(windows))]
    #[test]
    fn mock_windows_are_empty() {
        assert!(list_windows(None).unwrap().is_empty());
        assert_eq!(DwmThumbnails::default().count(), 0);
    }
}
