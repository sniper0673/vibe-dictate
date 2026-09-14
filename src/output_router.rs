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
    GetClientRect, GetCursorPos, GetForegroundWindow, GetWindowThreadProcessId, IsZoomed,
    SetCursorPos,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    Browser,
    Terminal,
    Claude,
    Generic,
}

#[derive(Debug, Clone)]
pub struct ForegroundTarget {
    pub kind: TargetKind,
    pub process_name: String,
}

pub fn detect_foreground_target() -> ForegroundTarget {
    let process_name = foreground_process_name().unwrap_or_else(|| "unknown".to_string());
    let kind = classify_process_name(&process_name);
    ForegroundTarget { kind, process_name }
}
pub fn classify_process_name(name: &str) -> TargetKind {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "chrome.exe" | "msedge.exe" | "brave.exe" | "vivaldi.exe" => TargetKind::Browser,
        "windowsterminal.exe" | "powershell.exe" | "pwsh.exe" | "cmd.exe" | "conhost.exe" => {
            TargetKind::Terminal
        }
        "claude.exe" => TargetKind::Claude,
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
    let hwnd: HWND = unsafe { GetForegroundWindow() };
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
