#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
}

pub type AppTheme = ThemeMode;

#[derive(Debug, Clone)]
pub struct ThemeColors {
    pub background: [f32; 3],
    pub surface: [f32; 3],
    pub text_primary: [f32; 3],
    pub text_secondary: [f32; 3],
    pub accent: [f32; 3],
    pub error: [f32; 3],
    pub success: [f32; 3],
    pub warning: [f32; 3],
}

impl ThemeColors {
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
        assert!(colors.background[0] < 0.5); // darker palette
    }

    #[test]
    fn light_theme() {
        let colors = ThemeColors::light();
        assert!(colors.background[0] > 0.5); // lighter palette
    }

    #[test]
    fn from_mode() {
        let dark = ThemeColors::from_mode(ThemeMode::Dark);
        let light = ThemeColors::from_mode(ThemeMode::Light);
        assert!(dark.background[0] < light.background[0]);
    }
}
