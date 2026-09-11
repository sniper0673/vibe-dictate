use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    Browser,
    Terminal,
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
        _ => TargetKind::Generic,
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
    Some(
        full.rsplit(['\\', '/'])
            .next()
            .unwrap_or(&full)
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_supported_targets() {
        assert_eq!(classify_process_name("chrome.exe"), TargetKind::Browser);
        assert_eq!(classify_process_name("msedge.exe"), TargetKind::Browser);
        assert_eq!(classify_process_name("WindowsTerminal.exe"), TargetKind::Terminal);
        assert_eq!(classify_process_name("pwsh.exe"), TargetKind::Terminal);
        assert_eq!(classify_process_name("notepad.exe"), TargetKind::Generic);
    }
}
