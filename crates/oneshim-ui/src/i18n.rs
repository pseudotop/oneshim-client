//! 국제화 (i18n) 모듈.
//!
//! 한국어(ko), 영어(en) 지원.

/// 지원 언어
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    /// 한국어 (기본값)
    #[default]
    Ko,
    /// 영어
    En,
}

impl Locale {
    /// 언어 코드 반환
    pub fn code(&self) -> &'static str {
        match self {
            Locale::Ko => "ko",
            Locale::En => "en",
        }
    }

    /// 언어 이름 (해당 언어로)
    pub fn name(&self) -> &'static str {
        match self {
            Locale::Ko => "한국어",
            Locale::En => "English",
        }
    }

    /// 시스템 로케일 감지
    pub fn detect_system() -> Self {
        // 환경 변수에서 언어 감지
        if let Ok(lang) = std::env::var("LANG") {
            if lang.starts_with("ko") {
                return Locale::Ko;
            }
        }
        if let Ok(lang) = std::env::var("LC_ALL") {
            if lang.starts_with("ko") {
                return Locale::Ko;
            }
        }
        // 기본값: 영어
        Locale::En
    }
}

/// UI 문자열 (로컬라이즈)
#[derive(Debug, Clone)]
pub struct Strings {
    // 앱 제목
    pub app_title: &'static str,
    pub app_title_settings: &'static str,

    // 네비게이션
    pub settings: &'static str,
    pub back: &'static str,
    pub quit: &'static str,

    // 대시보드
    pub connection_offline: &'static str,
    pub connection_disconnected: &'static str,
    pub connection_connected: &'static str,
    pub active_app: &'static str,
    pub system_metrics: &'static str,
    pub cpu: &'static str,
    pub memory: &'static str,
    pub agent: &'static str,
    pub system: &'static str,
    pub simple_view: &'static str,
    pub detail_view: &'static str,
    pub recent_suggestions: &'static str,
    pub no_suggestions: &'static str,
    pub update_status: &'static str,
    pub update_pending: &'static str,
    pub update_installing: &'static str,
    pub update_updated: &'static str,
    pub update_deferred: &'static str,
    pub update_error: &'static str,

    // 설정 섹션
    pub general: &'static str,
    pub startup: &'static str,
    pub appearance: &'static str,
    pub connection: &'static str,
    pub language: &'static str,

    // 설정 항목
    pub system_monitoring: &'static str,
    pub screenshot_capture: &'static str,
    pub desktop_notifications: &'static str,
    pub auto_start: &'static str,
    pub theme: &'static str,
    pub theme_dark: &'static str,
    pub theme_light: &'static str,
    pub server_url: &'static str,
    pub data_path: &'static str,
    pub version: &'static str,
}

impl Strings {
    /// 한국어 문자열
    pub const KO: Strings = Strings {
        app_title: "ONESHIM",
        app_title_settings: "ONESHIM - 설정",

        settings: "[설정]",
        back: "← 뒤로",
        quit: "종료",

        connection_offline: "오프라인 모드",
        connection_disconnected: "연결 안됨",
        connection_connected: "연결됨",
        active_app: "활성 앱",
        system_metrics: "# 클라이언트 리소스",
        cpu: "CPU",
        memory: "메모리",
        agent: "에이전트",
        system: "시스템",
        simple_view: "[간단히]",
        detail_view: "[상세]",
        recent_suggestions: "# 최근 제안",
        no_suggestions: "제안 없음",
        update_status: "업데이트",
        update_pending: "승인 대기",
        update_installing: "설치 중",
        update_updated: "업데이트 완료",
        update_deferred: "연기됨",
        update_error: "오류",

        general: "일반",
        startup: "시작",
        appearance: "외관",
        connection: "연결",
        language: "언어",

        system_monitoring: "시스템 모니터링",
        screenshot_capture: "스크린샷 캡처",
        desktop_notifications: "데스크톱 알림",
        auto_start: "로그인 시 자동 시작",
        theme: "테마",
        theme_dark: "다크",
        theme_light: "라이트",
        server_url: "서버 URL",
        data_path: "데이터 경로",
        version: "버전",
    };

    /// 영어 문자열
    pub const EN: Strings = Strings {
        app_title: "ONESHIM",
        app_title_settings: "ONESHIM - Settings",

        settings: "[Settings]",
        back: "← Back",
        quit: "Quit",

        connection_offline: "Offline Mode",
        connection_disconnected: "Disconnected",
        connection_connected: "Connected",
        active_app: "Active App",
        system_metrics: "# Client Resources",
        cpu: "CPU",
        memory: "Memory",
        agent: "Agent",
        system: "System",
        simple_view: "[Simple]",
        detail_view: "[Detail]",
        recent_suggestions: "# Recent Suggestions",
        no_suggestions: "No suggestions",
        update_status: "Update",
        update_pending: "Pending approval",
        update_installing: "Installing",
        update_updated: "Updated",
        update_deferred: "Deferred",
        update_error: "Error",

        general: "General",
        startup: "Startup",
        appearance: "Appearance",
        connection: "Connection",
        language: "Language",

        system_monitoring: "System Monitoring",
        screenshot_capture: "Screenshot Capture",
        desktop_notifications: "Desktop Notifications",
        auto_start: "Start at Login",
        theme: "Theme",
        theme_dark: "Dark",
        theme_light: "Light",
        server_url: "Server URL",
        data_path: "Data Path",
        version: "Version",
    };

    /// 로케일에 따른 문자열 반환
    pub fn for_locale(locale: Locale) -> &'static Strings {
        match locale {
            Locale::Ko => &Self::KO,
            Locale::En => &Self::EN,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_code() {
        assert_eq!(Locale::Ko.code(), "ko");
        assert_eq!(Locale::En.code(), "en");
    }

    #[test]
    fn locale_name() {
        assert_eq!(Locale::Ko.name(), "한국어");
        assert_eq!(Locale::En.name(), "English");
    }

    #[test]
    fn strings_for_locale() {
        let ko = Strings::for_locale(Locale::Ko);
        assert_eq!(ko.quit, "종료");

        let en = Strings::for_locale(Locale::En);
        assert_eq!(en.quit, "Quit");
    }

    #[test]
    fn default_locale_is_korean() {
        assert_eq!(Locale::default(), Locale::Ko);
    }
}
