use crate::{Error, Result};
use crate::tracker::app_tracker::window::ActiveWindowInfo;

use windows::core::{PWSTR, Result as WinResult};
use windows::Win32::Foundation::{CloseHandle, HANDLE, MAX_PATH};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    PROCESS_NAME_FORMAT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
};

pub fn get_active_window() -> Result<ActiveWindowInfo> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0 == 0 {
            return Ok(ActiveWindowInfo {
                window_title: "Desktop".into(),
                process_name: "explorer.exe".into(),
            });
        }

        // Window title
        let mut title_buf = [0u16; 512];
        let title_len = GetWindowTextW(hwnd, &mut title_buf);
        let mut title = String::from_utf16_lossy(&title_buf[..title_len as usize])
            .trim_end_matches('\0')
            .to_string();

        // Process ID + image path
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));

        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
            .map_err(|e| Error::Database(format!("OpenProcess failed: {}", e)))?; 

        let mut exe_buf = [0u16; MAX_PATH as usize];
        let mut buf_size: u32 = exe_buf.len() as u32;

        let query_result: WinResult<()> = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),  // 0 = Win32 path format
            PWSTR(exe_buf.as_mut_ptr()),
            &mut buf_size,
        );

        let _ = CloseHandle(handle);

        let process_path = match query_result {
            Ok(_) => {
                let len = buf_size as usize;
                // Remove trailing null terminator if present
                let slice = if len > 0 && exe_buf[len - 1] == 0 {
                    &exe_buf[..len - 1]
                } else {
                    &exe_buf[..len]
                };
                String::from_utf16_lossy(slice).to_string()
            }
            Err(e) => {
                log::warn!("QueryFullProcessImageNameW failed for pid {}: {}", pid, e);
                format!("unknown (pid {})", pid)
            }
        };

        let process_name = process_path
            .split('\\')
            .last()
            .unwrap_or("unknown.exe")
            .to_string();

        if title.is_empty() {
            title = process_name.clone();
        }

        Ok(ActiveWindowInfo {
            window_title: title,
            process_name,
        })
    }
}