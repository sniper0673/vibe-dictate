use std::ffi::c_void;
use std::thread;
use std::time::Duration;
use windows::core::PWSTR;

use anyhow::{anyhow, Result};
use windows::Win32::Foundation::{CloseHandle, HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetCursorPos, GetForegroundWindow, GetWindowThreadProcessId, IsIconic, IsWindow,
    IsZoomed, SetCursorPos, SetForegroundWindow, ShowWindow, SW_RESTORE,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    Browser,
    Terminal,
    Claude,
    OpenCode,
    Generic,
}

#[derive(Debug, Clone)]
pub struct ForegroundTarget {
    pub kind: TargetKind,
    pub process_name: String,
    /// Top-level foreground HWND captured when the utterance starts. Stored as an
    /// integer so the snapshot can safely cross the transcription worker thread.
    pub hwnd: isize,
}

pub fn detect_foreground_target() -> ForegroundTarget {
    let hwnd = unsafe { GetForegroundWindow() };
    let process_name = process_name_from_hwnd(hwnd).unwrap_or_else(|| "unknown".to_string());
    let kind = classify_process_name(&process_name);
    ForegroundTarget {
        kind,
        process_name,
        hwnd: hwnd.0 as isize,
    }
}

/// Temporarily reactivate the window captured at utterance start. The returned
/// guard restores whatever window the owner was using at delivery time.
pub struct ForegroundRestoreGuard {
    previous_hwnd: isize,
    target_hwnd: isize,
}

impl Drop for ForegroundRestoreGuard {
    fn drop(&mut self) {
        if self.previous_hwnd == 0 || self.previous_hwnd == self.target_hwnd {
            return;
        }
        let hwnd = HWND(self.previous_hwnd as *mut c_void);
        if unsafe { IsWindow(hwnd) }.as_bool() {
            let _ = unsafe { SetForegroundWindow(hwnd) };
        }
    }
}

pub fn activate_captured_target(target: &ForegroundTarget) -> Result<ForegroundRestoreGuard> {
    if target.hwnd == 0 {
        return Err(anyhow!("captured target has no window handle"));
    }
    let hwnd = HWND(target.hwnd as *mut c_void);
    if !unsafe { IsWindow(hwnd) }.as_bool() {
        return Err(anyhow!("captured target window no longer exists"));
    }
    let current_name = process_name_from_hwnd(hwnd)
        .ok_or_else(|| anyhow!("captured target process is unavailable"))?;
    if !current_name.eq_ignore_ascii_case(&target.process_name) {
        return Err(anyhow!(
            "captured target changed process: expected {}, got {}",
            target.process_name,
            current_name
        ));
    }

    let previous = unsafe { GetForegroundWindow() };
    if previous != hwnd {
        if unsafe { IsIconic(hwnd) }.as_bool() {
            let _ = unsafe { ShowWindow(hwnd, SW_RESTORE) };
        }
        if !unsafe { SetForegroundWindow(hwnd) }.as_bool() {
            return Err(anyhow!("could not reactivate captured target window"));
        }
        thread::sleep(Duration::from_millis(45));
        if unsafe { GetForegroundWindow() } != hwnd {
            if !previous.0.is_null() && unsafe { IsWindow(previous) }.as_bool() {
                let _ = unsafe { SetForegroundWindow(previous) };
            }
            return Err(anyhow!("captured target did not retain foreground focus"));
        }
    }
    Ok(ForegroundRestoreGuard {
        previous_hwnd: previous.0 as isize,
        target_hwnd: target.hwnd,
    })
}
pub fn classify_process_name(name: &str) -> TargetKind {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "chrome.exe" | "msedge.exe" | "brave.exe" | "vivaldi.exe" => TargetKind::Browser,
        "windowsterminal.exe" | "powershell.exe" | "pwsh.exe" | "cmd.exe" | "conhost.exe" => {
            TargetKind::Terminal
        }
        "claude.exe" => TargetKind::Claude,
        "opencode.exe" => TargetKind::OpenCode,
        _ => TargetKind::Generic,
    }
}

pub fn focus_claude_composer() -> Result<()> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return Err(anyhow!("no foreground window"));
    }
    let process_name = foreground_process_name().unwrap_or_else(|| "unknown".to_string());
    if classify_process_name(&process_name) != TargetKind::Claude {
        return Err(anyhow!("foreground target changed to {process_name}"));
    }
    if !unsafe { IsZoomed(hwnd) }.as_bool() {
        return Err(anyhow!("Claude window is not maximized"));
    }

    let mut client = RECT::default();
    unsafe { GetClientRect(hwnd, &mut client) }
        .map_err(|error| anyhow!("GetClientRect failed: {error}"))?;
    let width = client.right - client.left;
    let height = client.bottom - client.top;
    let (target_x, target_y) = claude_composer_client_point(width, height)?;

    let mut target = POINT {
        x: target_x,
        y: target_y,
    };
    if !unsafe { ClientToScreen(hwnd, &mut target) }.as_bool() {
        return Err(anyhow!("ClientToScreen failed"));
    }

    let mut previous = POINT::default();
    unsafe { GetCursorPos(&mut previous) }
        .map_err(|error| anyhow!("GetCursorPos failed: {error}"))?;
    unsafe { SetCursorPos(target.x, target.y) }
        .map_err(|error| anyhow!("SetCursorPos failed: {error}"))?;

    let inputs = [
        mouse_input(MOUSEEVENTF_LEFTDOWN),
        mouse_input(MOUSEEVENTF_LEFTUP),
    ];
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    let _ = unsafe { SetCursorPos(previous.x, previous.y) };
    if sent as usize != inputs.len() {
        return Err(anyhow!(
            "Claude composer click was only partially delivered: {sent}/{}",
            inputs.len()
        ));
    }
    thread::sleep(Duration::from_millis(80));
    Ok(())
}

fn claude_composer_client_point(width: i32, height: i32) -> Result<(i32, i32)> {
    if width < 800 || height < 600 {
        return Err(anyhow!(
            "Claude window is too small for safe composer targeting: {width}x{height}"
        ));
    }
    Ok((width / 2, height - 110))
}

fn mouse_input(flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn foreground_process_name() -> Option<String> {
    process_name_from_hwnd(unsafe { GetForegroundWindow() })
}

fn process_name_from_hwnd(hwnd: HWND) -> Option<String> {
    if hwnd.0.is_null() {
        return None;
    }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }
    process_name_from_pid(pid)
}
fn process_name_from_pid(pid: u32) -> Option<String> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buffer = vec![0u16; 32768];
    let mut size = buffer.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
    };
    unsafe {
        let _ = CloseHandle(handle);
    }
    if ok.is_err() || size == 0 {
        return None;
    }
    let full = String::from_utf16_lossy(&buffer[..size as usize]);
    Some(full.rsplit(['\\', '/']).next().unwrap_or(&full).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_supported_targets() {
        assert_eq!(classify_process_name("chrome.exe"), TargetKind::Browser);
        assert_eq!(classify_process_name("msedge.exe"), TargetKind::Browser);
        assert_eq!(
            classify_process_name("WindowsTerminal.exe"),
            TargetKind::Terminal
        );
        assert_eq!(classify_process_name("pwsh.exe"), TargetKind::Terminal);
        assert_eq!(classify_process_name("claude.exe"), TargetKind::Claude);
        assert_eq!(classify_process_name("OpenCode.exe"), TargetKind::OpenCode);
        assert_eq!(classify_process_name("notepad.exe"), TargetKind::Generic);
    }

    #[test]
    fn targets_bottom_center_of_large_claude_window() {
        assert_eq!(
            claude_composer_client_point(3840, 2078).unwrap(),
            (1920, 1968)
        );
    }

    #[test]
    fn rejects_small_claude_window() {
        assert!(claude_composer_client_point(799, 900).is_err());
        assert!(claude_composer_client_point(1200, 599).is_err());
    }
}
