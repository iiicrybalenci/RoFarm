use crate::error::{PlatformError, PlatformResult};
use crate::model::WindowId;

pub fn send_anti_afk(hwnd: WindowId, foreground: bool) -> PlatformResult<()> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP, VK_SHIFT};
        use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, SetForegroundWindow, WM_KEYDOWN, WM_KEYUP};
        const WM_KEYDOWN_U32: u32 = 0x0100;
        const WM_KEYUP_U32: u32 = 0x0101;
        let _ = (WM_KEYDOWN, WM_KEYUP);
        if foreground {
            if hwnd.is_null() {
                return Err(PlatformError::Invalid("no client window".into()));
            }
            unsafe {
                SetForegroundWindow(hwnd.as_isize() as _);
                keybd_event(VK_SHIFT as u8, 0, 0, 0);
                keybd_event(VK_SHIFT as u8, 0, KEYEVENTF_KEYUP, 0);
            }
            Ok(())
        } else {
            if hwnd.is_null() {
                return Err(PlatformError::Invalid("no client window".into()));
            }
            let posted = unsafe {
                PostMessageW(hwnd.as_isize() as _, WM_KEYDOWN_U32, VK_SHIFT as usize, 0)
                    & PostMessageW(hwnd.as_isize() as _, WM_KEYUP_U32, VK_SHIFT as usize, 0)
            };
            if posted == 0 {
                return Err(PlatformError::Win32("PostMessageW"));
            }
            Ok(())
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (hwnd, foreground);
        Err(PlatformError::Unsupported)
    }
}
