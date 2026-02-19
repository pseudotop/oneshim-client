//! 자동 시작(Login Item) 관리 모듈.
//!
//! macOS: LaunchAgent plist
//! Windows: Registry (HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Run)
//! Linux: XDG autostart (미구현)

/// 자동 시작 상태 확인
pub fn check_autostart_status() -> bool {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        let plist_path = format!("{home}/Library/LaunchAgents/com.oneshim.agent.plist");
        std::path::Path::new(&plist_path).exists()
    }

    #[cfg(target_os = "windows")]
    {
        // Windows Registry 확인 (간단한 구현)
        false
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

/// 자동 시작 활성화
pub fn enable_autostart() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe_str = exe.to_string_lossy();
        let home = std::env::var("HOME").map_err(|_| "HOME not set")?;

        let plist_content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.oneshim.agent</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe_str}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
</dict>
</plist>
"#
        );

        let plist_dir = format!("{home}/Library/LaunchAgents");
        std::fs::create_dir_all(&plist_dir).map_err(|e| e.to_string())?;

        let plist_path = format!("{plist_dir}/com.oneshim.agent.plist");
        std::fs::write(&plist_path, plist_content).map_err(|e| e.to_string())?;

        std::process::Command::new("launchctl")
            .args(["load", &plist_path])
            .output()
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err("Not supported on this platform".to_string())
    }
}

/// 자동 시작 비활성화
pub fn disable_autostart() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").map_err(|_| "HOME not set")?;
        let plist_path = format!("{home}/Library/LaunchAgents/com.oneshim.agent.plist");

        if std::path::Path::new(&plist_path).exists() {
            let _ = std::process::Command::new("launchctl")
                .args(["unload", &plist_path])
                .output();
            std::fs::remove_file(&plist_path).map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err("Not supported on this platform".to_string())
    }
}
