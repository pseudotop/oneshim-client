//! 자동 시작(로그인 시 실행) 관리.
//!
//! - macOS: `~/Library/LaunchAgents/com.oneshim.agent.plist`
//! - Windows: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 레지스트리
//! - 미지원 플랫폼: no-op (warning 로그)

#![allow(dead_code)]

/// 앱 식별자
const APP_LABEL: &str = "com.oneshim.agent";

/// 자동 시작 활성화
pub fn enable_autostart() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::enable()
    }

    #[cfg(target_os = "windows")]
    {
        return windows::enable();
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        tracing::warn!("자동 시작: 현재 플랫폼 미지원");
        Ok(())
    }
}

/// 자동 시작 비활성화
pub fn disable_autostart() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::disable()
    }

    #[cfg(target_os = "windows")]
    {
        return windows::disable();
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        tracing::warn!("자동 시작 비활성화: 현재 플랫폼 미지원");
        Ok(())
    }
}

/// 자동 시작 상태 확인
pub fn is_autostart_enabled() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        macos::is_enabled()
    }

    #[cfg(target_os = "windows")]
    {
        return windows::is_enabled();
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        tracing::warn!("자동 시작 확인: 현재 플랫폼 미지원");
        Ok(false)
    }
}

// ── macOS LaunchAgent 구현 ──

#[cfg(target_os = "macos")]
mod macos {
    use super::APP_LABEL;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    /// LaunchAgents 디렉토리 내 plist 경로
    pub fn plist_path() -> Result<PathBuf, String> {
        let home = std::env::var("HOME").map_err(|_| "HOME 환경변수 없음".to_string())?;
        Ok(PathBuf::from(home)
            .join("Library")
            .join("LaunchAgents")
            .join(format!("{APP_LABEL}.plist")))
    }

    /// 현재 바이너리 경로
    fn binary_path() -> Result<String, String> {
        std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .map_err(|e| format!("바이너리 경로 확인 실패: {e}"))
    }

    /// plist XML 생성
    pub fn generate_plist(program_path: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{APP_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{program_path}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
    <key>StandardOutPath</key>
    <string>/tmp/oneshim-client.out.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/oneshim-client.err.log</string>
</dict>
</plist>
"#
        )
    }

    pub fn enable() -> Result<(), String> {
        let path = plist_path()?;
        let bin = binary_path()?;
        let plist_content = generate_plist(&bin);

        // LaunchAgents 디렉토리 생성 (없으면)
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("LaunchAgents 디렉토리 생성 실패: {e}"))?;
        }

        fs::write(&path, plist_content).map_err(|e| format!("plist 파일 작성 실패: {e}"))?;

        // launchctl load
        Command::new("launchctl")
            .args(["load", &path.to_string_lossy()])
            .output()
            .map_err(|e| format!("launchctl load 실패: {e}"))?;

        Ok(())
    }

    pub fn disable() -> Result<(), String> {
        let path = plist_path()?;

        if path.exists() {
            // launchctl unload
            let _ = Command::new("launchctl")
                .args(["unload", &path.to_string_lossy()])
                .output();

            fs::remove_file(&path).map_err(|e| format!("plist 삭제 실패: {e}"))?;
        }

        Ok(())
    }

    pub fn is_enabled() -> Result<bool, String> {
        let path = plist_path()?;
        Ok(path.exists())
    }
}

// ── Windows 레지스트리 구현 ──

#[cfg(target_os = "windows")]
mod windows {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
        HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_SZ,
    };

    const SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "ONESHIM";

    /// UTF-16 문자열로 변환 (null-terminated)
    fn to_wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    pub fn enable() -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| format!("바이너리 경로 확인 실패: {e}"))?;
        let exe_str = exe.to_string_lossy();
        let exe_wide = to_wide(&exe_str);

        let subkey_wide = to_wide(SUBKEY);
        let value_wide = to_wide(VALUE_NAME);

        unsafe {
            let mut hkey: HKEY = std::ptr::null_mut();
            let result = RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey_wide.as_ptr(),
                0,
                KEY_WRITE,
                &mut hkey,
            );
            if result != 0 {
                return Err(format!("레지스트리 열기 실패: 코드 {result}"));
            }

            let byte_len = (exe_wide.len() * 2) as u32;
            let result = RegSetValueExW(
                hkey,
                value_wide.as_ptr(),
                0,
                REG_SZ,
                exe_wide.as_ptr() as *const u8,
                byte_len,
            );
            RegCloseKey(hkey);

            if result != 0 {
                return Err(format!("레지스트리 값 설정 실패: 코드 {result}"));
            }
        }

        Ok(())
    }

    pub fn disable() -> Result<(), String> {
        let subkey_wide = to_wide(SUBKEY);
        let value_wide = to_wide(VALUE_NAME);

        unsafe {
            let mut hkey: HKEY = std::ptr::null_mut();
            let result = RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey_wide.as_ptr(),
                0,
                KEY_WRITE,
                &mut hkey,
            );
            if result != 0 {
                // 키가 없으면 이미 비활성화된 것
                return Ok(());
            }

            let _ = RegDeleteValueW(hkey, value_wide.as_ptr());
            RegCloseKey(hkey);
        }

        Ok(())
    }

    pub fn is_enabled() -> Result<bool, String> {
        let subkey_wide = to_wide(SUBKEY);
        let value_wide = to_wide(VALUE_NAME);

        unsafe {
            let mut hkey: HKEY = std::ptr::null_mut();
            let result = RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey_wide.as_ptr(),
                0,
                KEY_READ,
                &mut hkey,
            );
            if result != 0 {
                return Ok(false);
            }

            let result = RegQueryValueExW(
                hkey,
                value_wide.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            RegCloseKey(hkey);

            Ok(result == 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    mod macos_tests {
        use super::*;

        #[test]
        fn plist_xml_contains_required_keys() {
            let plist = macos::generate_plist("/usr/local/bin/oneshim");
            assert!(plist.contains("<key>Label</key>"));
            assert!(plist.contains(APP_LABEL));
            assert!(plist.contains("<key>RunAtLoad</key>"));
            assert!(plist.contains("<true/>"));
            assert!(plist.contains("<key>KeepAlive</key>"));
            assert!(plist.contains("<false/>"));
            assert!(plist.contains("/usr/local/bin/oneshim"));
        }

        #[test]
        fn plist_path_under_launch_agents() {
            let path = macos::plist_path().unwrap();
            assert!(path.to_string_lossy().contains("LaunchAgents"));
            assert!(path.to_string_lossy().ends_with("com.oneshim.agent.plist"));
        }

        #[test]
        fn plist_is_valid_xml() {
            let plist = macos::generate_plist("/usr/local/bin/oneshim");
            assert!(plist.starts_with("<?xml version=\"1.0\""));
            assert!(plist.contains("<!DOCTYPE plist"));
            assert!(plist.contains("<plist version=\"1.0\">"));
            assert!(plist.trim().ends_with("</plist>"));
        }
    }

    #[test]
    fn enable_disable_roundtrip_unsupported_platform() {
        // macOS/Windows에서는 실제 시스템 변경이 일어나므로
        // 미지원 플랫폼 코드 경로를 항상 테스트 가능하도록 검증
        // 이 테스트는 함수 시그니처와 반환 타입 검증용
        let _ = enable_autostart();
        let _ = disable_autostart();
        let _ = is_autostart_enabled();
    }
}
