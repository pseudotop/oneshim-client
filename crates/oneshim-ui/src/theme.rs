//! UI 테마 정의.
//!
//! 다크/라이트 테마 색상.

/// 테마 모드
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    /// 다크 테마
    Dark,
    /// 라이트 테마
    Light,
}

/// 앱 테마 (ThemeMode 별칭)
pub type AppTheme = ThemeMode;

/// 테마 색상 팔레트
#[derive(Debug, Clone)]
pub struct ThemeColors {
    /// 배경색
    pub background: [f32; 3],
    /// 표면 색
    pub surface: [f32; 3],
    /// 주요 텍스트 색
    pub text_primary: [f32; 3],
    /// 보조 텍스트 색
    pub text_secondary: [f32; 3],
    /// 강조색
    pub accent: [f32; 3],
    /// 에러 색
    pub error: [f32; 3],
    /// 성공 색
    pub success: [f32; 3],
    /// 경고 색
    pub warning: [f32; 3],
}

impl ThemeColors {
    /// 다크 테마 팔레트
    pub fn dark() -> Self {
        Self {
            background: [0.11, 0.11, 0.12],   // #1C1C1F
            surface: [0.16, 0.16, 0.18],      // #292930
            text_primary: [0.95, 0.95, 0.96], // #F2F2F5
            text_secondary: [0.6, 0.6, 0.65], // #9999A6
            accent: [0.23, 0.51, 0.96],       // #3B82F6
            error: [0.94, 0.27, 0.27],        // #EF4444
            success: [0.13, 0.77, 0.47],      // #22C578
            warning: [0.98, 0.45, 0.09],      // #F97316
        }
    }

    /// 라이트 테마 팔레트
    pub fn light() -> Self {
        Self {
            background: [0.98, 0.98, 0.98],     // #FAFAFA
            surface: [1.0, 1.0, 1.0],           // #FFFFFF
            text_primary: [0.1, 0.1, 0.12],     // #1A1A1F
            text_secondary: [0.42, 0.42, 0.47], // #6B6B78
            accent: [0.23, 0.51, 0.96],         // #3B82F6
            error: [0.94, 0.27, 0.27],          // #EF4444
            success: [0.13, 0.77, 0.47],        // #22C578
            warning: [0.98, 0.45, 0.09],        // #F97316
        }
    }

    /// 모드에 따른 팔레트 반환
    pub fn from_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Dark => Self::dark(),
            ThemeMode::Light => Self::light(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_theme() {
        let colors = ThemeColors::dark();
        assert!(colors.background[0] < 0.5); // 어두운 배경
    }

    #[test]
    fn light_theme() {
        let colors = ThemeColors::light();
        assert!(colors.background[0] > 0.5); // 밝은 배경
    }

    #[test]
    fn from_mode() {
        let dark = ThemeColors::from_mode(ThemeMode::Dark);
        let light = ThemeColors::from_mode(ThemeMode::Light);
        assert!(dark.background[0] < light.background[0]);
    }
}
