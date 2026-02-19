//! 작업 세션 및 중단 관련 모델.
//!
//! 소통 비용 감소를 위한 핵심 모델:
//! - 앱 카테고리 분류
//! - 작업 세션 감지
//! - 중단 컨텍스트 추적
//! - 집중 메트릭

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 앱 카테고리 분류
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AppCategory {
    /// 소통 앱 (Slack, Teams, Mail, Meet, Zoom)
    Communication,
    /// 개발 도구 (VS Code, IntelliJ, Terminal, Git)
    Development,
    /// 문서 작업 (Notion, Confluence, Word, Excel)
    Documentation,
    /// 브라우저 (Chrome, Safari, Firefox)
    Browser,
    /// 디자인 (Figma, Sketch, Photoshop)
    Design,
    /// 미디어 (Spotify, YouTube, Netflix)
    Media,
    /// 시스템 (Finder, Explorer, Settings)
    System,
    /// 기타
    #[default]
    Other,
}

impl AppCategory {
    /// 앱 이름으로 카테고리 추론
    pub fn from_app_name(app_name: &str) -> Self {
        let name = app_name.to_lowercase();

        // 소통 앱
        if name.contains("slack")
            || name.contains("teams")
            || name.contains("discord")
            || name.contains("zoom")
            || name.contains("meet")
            || name.contains("mail")
            || name.contains("outlook")
            || name.contains("gmail")
            || name.contains("messages")
            || name.contains("kakaotalk")
            || name.contains("telegram")
            || name.contains("whatsapp")
        {
            return Self::Communication;
        }

        // 개발 도구
        if name.contains("code")
            || name.contains("visual studio")
            || name.contains("intellij")
            || name.contains("pycharm")
            || name.contains("webstorm")
            || name.contains("android studio")
            || name.contains("xcode")
            || name.contains("terminal")
            || name.contains("iterm")
            || name.contains("warp")
            || name.contains("git")
            || name.contains("sourcetree")
            || name.contains("postman")
            || name.contains("insomnia")
        {
            return Self::Development;
        }

        // 문서 작업
        if name.contains("notion")
            || name.contains("confluence")
            || name.contains("word")
            || name.contains("excel")
            || name.contains("powerpoint")
            || name.contains("pages")
            || name.contains("numbers")
            || name.contains("keynote")
            || name.contains("google docs")
            || name.contains("obsidian")
            || name.contains("typora")
        {
            return Self::Documentation;
        }

        // 브라우저
        if name.contains("chrome")
            || name.contains("safari")
            || name.contains("firefox")
            || name.contains("edge")
            || name.contains("arc")
            || name.contains("brave")
        {
            return Self::Browser;
        }

        // 디자인
        if name.contains("figma")
            || name.contains("sketch")
            || name.contains("photoshop")
            || name.contains("illustrator")
            || name.contains("canva")
        {
            return Self::Design;
        }

        // 미디어
        if name.contains("spotify")
            || name.contains("music")
            || name.contains("youtube")
            || name.contains("netflix")
            || name.contains("vlc")
        {
            return Self::Media;
        }

        // 시스템
        if name.contains("finder")
            || name.contains("explorer")
            || name.contains("settings")
            || name.contains("system preferences")
            || name.contains("activity monitor")
            || name.contains("task manager")
        {
            return Self::System;
        }

        Self::Other
    }

    /// 소통 앱인지 확인
    pub fn is_communication(&self) -> bool {
        matches!(self, Self::Communication)
    }

    /// 깊은 작업 앱인지 확인 (개발, 문서, 디자인)
    pub fn is_deep_work(&self) -> bool {
        matches!(self, Self::Development | Self::Documentation | Self::Design)
    }

    /// 한글 레이블
    pub fn label_ko(&self) -> &'static str {
        match self {
            Self::Communication => "소통",
            Self::Development => "개발",
            Self::Documentation => "문서",
            Self::Browser => "브라우저",
            Self::Design => "디자인",
            Self::Media => "미디어",
            Self::System => "시스템",
            Self::Other => "기타",
        }
    }
}

/// 작업 세션 상태
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// 진행 중
    Active,
    /// 유휴로 종료
    EndedByIdle,
    /// 앱 전환으로 종료
    EndedBySwitch,
}

/// 작업 세션
///
/// 동일 앱/카테고리에서의 연속 작업 기간
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkSession {
    /// 세션 ID
    pub id: i64,
    /// 시작 시각
    pub started_at: DateTime<Utc>,
    /// 종료 시각 (None이면 진행 중)
    pub ended_at: Option<DateTime<Utc>>,
    /// 주 앱 이름
    pub primary_app: String,
    /// 앱 카테고리
    pub category: AppCategory,
    /// 세션 상태
    pub state: SessionState,
    /// 중단 횟수 (소통 앱으로 전환)
    pub interruption_count: u32,
    /// 깊은 작업 시간 (초) - 연속 5분 이상 집중
    pub deep_work_secs: u64,
    /// 총 지속 시간 (초)
    pub duration_secs: u64,
}

impl WorkSession {
    /// 새 세션 시작
    pub fn new(id: i64, app_name: String) -> Self {
        let category = AppCategory::from_app_name(&app_name);
        Self {
            id,
            started_at: Utc::now(),
            ended_at: None,
            primary_app: app_name,
            category,
            state: SessionState::Active,
            interruption_count: 0,
            deep_work_secs: 0,
            duration_secs: 0,
        }
    }

    /// 세션 진행 중인지 확인
    pub fn is_active(&self) -> bool {
        self.state == SessionState::Active
    }

    /// 집중도 점수 (0.0 ~ 1.0)
    ///
    /// 중단이 적고 깊은 작업 비율이 높을수록 높음
    pub fn focus_score(&self) -> f32 {
        if self.duration_secs == 0 {
            return 0.0;
        }

        let deep_work_ratio = self.deep_work_secs as f32 / self.duration_secs as f32;
        let interruption_penalty = (self.interruption_count as f32 * 0.1).min(0.5);

        (deep_work_ratio - interruption_penalty).clamp(0.0, 1.0)
    }
}

/// 중단 이벤트
///
/// 깊은 작업 중 소통 앱으로 전환된 경우
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interruption {
    /// 중단 ID
    pub id: i64,
    /// 중단 시각
    pub interrupted_at: DateTime<Utc>,
    /// 중단된 앱
    pub from_app: String,
    /// 중단된 앱 카테고리
    pub from_category: AppCategory,
    /// 전환된 앱 (소통 앱)
    pub to_app: String,
    /// 전환된 앱 카테고리
    pub to_category: AppCategory,
    /// 중단 시점 프레임 ID (스냅샷)
    pub snapshot_frame_id: Option<i64>,
    /// 복귀 시각
    pub resumed_at: Option<DateTime<Utc>>,
    /// 복귀한 앱 (원래 앱으로 돌아왔는지)
    pub resumed_to_app: Option<String>,
    /// 중단 지속 시간 (초)
    pub duration_secs: Option<u64>,
}

impl Interruption {
    /// 새 중단 이벤트 생성
    pub fn new(id: i64, from_app: String, to_app: String, snapshot_frame_id: Option<i64>) -> Self {
        Self {
            id,
            interrupted_at: Utc::now(),
            from_category: AppCategory::from_app_name(&from_app),
            from_app,
            to_category: AppCategory::from_app_name(&to_app),
            to_app,
            snapshot_frame_id,
            resumed_at: None,
            resumed_to_app: None,
            duration_secs: None,
        }
    }

    /// 복귀 기록
    pub fn mark_resumed(&mut self, resumed_to_app: String) {
        let now = Utc::now();
        self.resumed_at = Some(now);
        self.resumed_to_app = Some(resumed_to_app);
        self.duration_secs = Some((now - self.interrupted_at).num_seconds() as u64);
    }

    /// 원래 앱으로 복귀했는지 확인
    pub fn resumed_to_original(&self) -> bool {
        self.resumed_to_app
            .as_ref()
            .map(|app| app == &self.from_app)
            .unwrap_or(false)
    }
}

/// 집중 메트릭 (시간 단위)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusMetrics {
    /// 측정 시작 시각
    pub period_start: DateTime<Utc>,
    /// 측정 종료 시각
    pub period_end: DateTime<Utc>,
    /// 총 활동 시간 (초)
    pub total_active_secs: u64,
    /// 깊은 작업 시간 (초)
    pub deep_work_secs: u64,
    /// 소통 시간 (초)
    pub communication_secs: u64,
    /// 컨텍스트 스위치 횟수
    pub context_switches: u32,
    /// 중단 횟수 (깊은 작업 → 소통)
    pub interruption_count: u32,
    /// 평균 집중 시간 (연속 깊은 작업, 초)
    pub avg_focus_duration_secs: u64,
    /// 최장 집중 시간 (초)
    pub max_focus_duration_secs: u64,
    /// 집중도 점수 (0.0 ~ 1.0)
    pub focus_score: f32,
}

impl FocusMetrics {
    /// 새 메트릭 생성
    pub fn new(period_start: DateTime<Utc>, period_end: DateTime<Utc>) -> Self {
        Self {
            period_start,
            period_end,
            total_active_secs: 0,
            deep_work_secs: 0,
            communication_secs: 0,
            context_switches: 0,
            interruption_count: 0,
            avg_focus_duration_secs: 0,
            max_focus_duration_secs: 0,
            focus_score: 0.0,
        }
    }

    /// 소통 비율 (0.0 ~ 1.0)
    pub fn communication_ratio(&self) -> f32 {
        if self.total_active_secs == 0 {
            return 0.0;
        }
        self.communication_secs as f32 / self.total_active_secs as f32
    }

    /// 깊은 작업 비율 (0.0 ~ 1.0)
    pub fn deep_work_ratio(&self) -> f32 {
        if self.total_active_secs == 0 {
            return 0.0;
        }
        self.deep_work_secs as f32 / self.total_active_secs as f32
    }

    /// 시간당 중단 횟수
    pub fn interruptions_per_hour(&self) -> f32 {
        let hours = (self.period_end - self.period_start).num_seconds() as f32 / 3600.0;
        if hours == 0.0 {
            return 0.0;
        }
        self.interruption_count as f32 / hours
    }
}

/// 카테고리별 사용 시간
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryUsage {
    /// 카테고리
    pub category: AppCategory,
    /// 사용 시간 (초)
    pub duration_secs: u64,
    /// 사용 비율 (0.0 ~ 1.0)
    pub ratio: f32,
    /// 세션 수
    pub session_count: u32,
}

/// 로컬 제안 타입
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LocalSuggestion {
    /// 집중 시간 필요
    NeedFocusTime {
        communication_ratio: f32,
        suggested_focus_mins: u32,
    },
    /// 휴식 권장
    TakeBreak { continuous_work_mins: u32 },
    /// 컨텍스트 복원
    RestoreContext {
        interrupted_app: String,
        interrupted_at: DateTime<Utc>,
        snapshot_frame_id: i64,
    },
    /// 반복 패턴 감지
    PatternDetected {
        pattern_description: String,
        confidence: f32,
    },
    /// 소통 과다
    ExcessiveCommunication {
        today_communication_mins: u32,
        avg_communication_mins: u32,
    },
}

impl LocalSuggestion {
    /// 제안 우선순위 (높을수록 중요)
    pub fn priority(&self) -> u8 {
        match self {
            Self::RestoreContext { .. } => 100, // 즉시 필요
            Self::TakeBreak { .. } => 80,
            Self::NeedFocusTime { .. } => 60,
            Self::ExcessiveCommunication { .. } => 40,
            Self::PatternDetected { .. } => 20,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_category_from_name() {
        assert_eq!(
            AppCategory::from_app_name("Slack"),
            AppCategory::Communication
        );
        assert_eq!(
            AppCategory::from_app_name("Visual Studio Code"),
            AppCategory::Development
        );
        assert_eq!(
            AppCategory::from_app_name("Google Chrome"),
            AppCategory::Browser
        );
        assert_eq!(
            AppCategory::from_app_name("Notion"),
            AppCategory::Documentation
        );
        assert_eq!(AppCategory::from_app_name("Figma"), AppCategory::Design);
        assert_eq!(
            AppCategory::from_app_name("Unknown App"),
            AppCategory::Other
        );
    }

    #[test]
    fn app_category_is_communication() {
        assert!(AppCategory::Communication.is_communication());
        assert!(!AppCategory::Development.is_communication());
    }

    #[test]
    fn app_category_is_deep_work() {
        assert!(AppCategory::Development.is_deep_work());
        assert!(AppCategory::Documentation.is_deep_work());
        assert!(!AppCategory::Communication.is_deep_work());
        assert!(!AppCategory::Browser.is_deep_work());
    }

    #[test]
    fn work_session_focus_score() {
        let mut session = WorkSession::new(1, "Code".to_string());
        session.duration_secs = 3600; // 1시간
        session.deep_work_secs = 3000; // 50분 집중
        session.interruption_count = 2;

        let score = session.focus_score();
        // deep_work_ratio = 3000/3600 = 0.833
        // interruption_penalty = 2 * 0.1 = 0.2
        // score = 0.833 - 0.2 = 0.633
        assert!(score > 0.6 && score < 0.7);
    }

    #[test]
    fn interruption_resumed_to_original() {
        let mut interruption =
            Interruption::new(1, "Code".to_string(), "Slack".to_string(), Some(100));

        assert!(!interruption.resumed_to_original());

        interruption.mark_resumed("Code".to_string());
        assert!(interruption.resumed_to_original());
    }

    #[test]
    fn focus_metrics_ratios() {
        let now = Utc::now();
        let mut metrics = FocusMetrics::new(now, now + chrono::Duration::hours(1));
        metrics.total_active_secs = 3600;
        metrics.deep_work_secs = 2400; // 40분
        metrics.communication_secs = 1200; // 20분

        assert!((metrics.deep_work_ratio() - 0.667).abs() < 0.01);
        assert!((metrics.communication_ratio() - 0.333).abs() < 0.01);
    }

    #[test]
    fn local_suggestion_priority() {
        let restore = LocalSuggestion::RestoreContext {
            interrupted_app: "Code".to_string(),
            interrupted_at: Utc::now(),
            snapshot_frame_id: 1,
        };
        let break_suggestion = LocalSuggestion::TakeBreak {
            continuous_work_mins: 120,
        };

        assert!(restore.priority() > break_suggestion.priority());
    }
}
