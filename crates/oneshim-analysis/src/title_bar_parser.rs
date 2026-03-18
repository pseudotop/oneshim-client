use oneshim_core::models::tiered_memory::{ContainerType, ContentType};

/// Extracted content information from a window title bar.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedContent {
    pub content_label: String,
    pub content_type: ContentType,
    pub confidence: f32,
}

/// Known container applications (RDP / VM / VNC / Citrix).
const CONTAINER_APPS: &[(&str, ContainerType)] = &[
    ("microsoft remote desktop", ContainerType::Rdp),
    ("windows app", ContainerType::Rdp),
    ("freerdp", ContainerType::Rdp),
    ("parallels desktop", ContainerType::Vm),
    ("vmware fusion", ContainerType::Vm),
    ("virtualbox", ContainerType::Vm),
    ("utm", ContainerType::Vm),
    ("vnc viewer", ContainerType::Vnc),
    ("realvnc", ContainerType::Vnc),
    ("citrix workspace", ContainerType::Citrix),
];

/// Parse window title bars to extract content labels and types.
///
/// Each app family has its own title format convention. The parser tries
/// app-specific patterns first, falling back to a generic splitter.
pub struct TitleBarParser;

impl TitleBarParser {
    pub fn new() -> Self {
        Self
    }

    /// Parse window title to extract content label and type.
    pub fn parse(&self, app_name: &str, window_title: &str) -> Option<ParsedContent> {
        if window_title.is_empty() {
            return None;
        }

        let lower_app = app_name.to_lowercase();

        // Try app-specific patterns first
        if let Some(result) = self.parse_ide(&lower_app, window_title) {
            return Some(result);
        }
        if let Some(result) = self.parse_browser(&lower_app, window_title) {
            return Some(result);
        }
        if let Some(result) = self.parse_communication(&lower_app, window_title) {
            return Some(result);
        }
        if let Some(result) = self.parse_terminal(&lower_app, window_title) {
            return Some(result);
        }
        if let Some(result) = self.parse_document(&lower_app, window_title) {
            return Some(result);
        }
        if let Some(result) = self.parse_design(&lower_app, window_title) {
            return Some(result);
        }

        // Generic: use first segment before " - " as content
        self.parse_generic(window_title)
    }

    /// Check if app is a known container (RDP/VM/VNC/Citrix).
    pub fn is_container_app(&self, app_name: &str) -> Option<ContainerType> {
        let lower = app_name.to_lowercase();
        for (pattern, container_type) in CONTAINER_APPS {
            if lower.contains(pattern) {
                return Some(*container_type);
            }
        }
        None
    }

    /// VSCode/Cursor: `{file} - {project} - Visual Studio Code`
    /// IntelliJ/WebStorm: `{project} – {file}` (em-dash)
    fn parse_ide(&self, lower_app: &str, title: &str) -> Option<ParsedContent> {
        let is_vscode = lower_app.contains("code") || lower_app.contains("cursor");
        let is_jetbrains = lower_app.contains("intellij")
            || lower_app.contains("webstorm")
            || lower_app.contains("pycharm")
            || lower_app.contains("android studio")
            || lower_app.contains("goland")
            || lower_app.contains("rider")
            || lower_app.contains("rustrover")
            || lower_app.contains("clion");

        if is_vscode {
            // Format: "{file} - {project} - Visual Studio Code"
            // or:     "{file} - {project} - Cursor"
            let parts: Vec<&str> = title.split(" - ").collect();
            if parts.len() >= 2 {
                let file = parts[0].trim();
                if !file.is_empty() {
                    return Some(ParsedContent {
                        content_label: file.to_string(),
                        content_type: ContentType::File,
                        confidence: 0.95,
                    });
                }
            }
        }

        if is_jetbrains {
            // Format: "{project} – {file}" (em-dash \u{2013})
            if let Some(idx) = title.find('\u{2013}') {
                let file = title[idx + '\u{2013}'.len_utf8()..].trim();
                if !file.is_empty() {
                    return Some(ParsedContent {
                        content_label: file.to_string(),
                        content_type: ContentType::File,
                        confidence: 0.90,
                    });
                }
            }
            // Fallback: some JetBrains IDEs use " - " instead
            // Format: "{project} - {file} - {IDE}" or "{project} - {file}"
            let parts: Vec<&str> = title.split(" - ").collect();
            if parts.len() >= 3 {
                // {project} - {file} - {IDE}
                let file = parts[1].trim();
                if !file.is_empty() {
                    return Some(ParsedContent {
                        content_label: file.to_string(),
                        content_type: ContentType::File,
                        confidence: 0.70,
                    });
                }
            } else if parts.len() == 2 {
                // {project} - {file} or {file} - {IDE}
                let file = parts[0].trim();
                if !file.is_empty() {
                    return Some(ParsedContent {
                        content_label: file.to_string(),
                        content_type: ContentType::File,
                        confidence: 0.60,
                    });
                }
            }
        }

        None
    }

    /// Chrome/Arc/Edge/Firefox/Safari/Brave: `{page_title} - {browser}`
    fn parse_browser(&self, lower_app: &str, title: &str) -> Option<ParsedContent> {
        let is_browser = lower_app.contains("chrome")
            || lower_app.contains("arc")
            || lower_app.contains("edge")
            || lower_app.contains("firefox")
            || lower_app.contains("safari")
            || lower_app.contains("brave")
            || lower_app.contains("opera");

        if !is_browser {
            return None;
        }

        // Format: "{page_title} - {browser_name}"
        // Take everything before the last " - " separator
        if let Some(idx) = title.rfind(" - ") {
            let page = title[..idx].trim();
            if !page.is_empty() {
                return Some(ParsedContent {
                    content_label: page.to_string(),
                    content_type: ContentType::WebPage,
                    confidence: 0.90,
                });
            }
        }

        // No separator — use entire title
        Some(ParsedContent {
            content_label: title.to_string(),
            content_type: ContentType::WebPage,
            confidence: 0.70,
        })
    }

    /// Slack: `{workspace} - {channel}` or `{channel} | {workspace}`
    fn parse_communication(&self, lower_app: &str, title: &str) -> Option<ParsedContent> {
        let is_slack = lower_app.contains("slack");
        let is_teams = lower_app.contains("teams");
        let is_discord = lower_app.contains("discord");

        if is_slack {
            // Slack: "{channel} | {workspace}" or "{workspace} - {channel}"
            if let Some(idx) = title.find(" | ") {
                let channel = title[..idx].trim();
                if !channel.is_empty() {
                    return Some(ParsedContent {
                        content_label: channel.to_string(),
                        content_type: ContentType::Channel,
                        confidence: 0.95,
                    });
                }
            }
            if let Some(idx) = title.find(" - ") {
                let channel = title[idx + 3..].trim();
                if !channel.is_empty() {
                    return Some(ParsedContent {
                        content_label: channel.to_string(),
                        content_type: ContentType::Channel,
                        confidence: 0.90,
                    });
                }
            }
        }

        if is_teams || is_discord {
            // Teams/Discord: "{channel/chat} - {app}" or "{channel} | {server}"
            if let Some(idx) = title.find(" | ") {
                let channel = title[..idx].trim();
                if !channel.is_empty() {
                    return Some(ParsedContent {
                        content_label: channel.to_string(),
                        content_type: ContentType::Channel,
                        confidence: 0.85,
                    });
                }
            }
            if let Some(idx) = title.find(" - ") {
                let channel = title[..idx].trim();
                if !channel.is_empty() {
                    return Some(ParsedContent {
                        content_label: channel.to_string(),
                        content_type: ContentType::Channel,
                        confidence: 0.80,
                    });
                }
            }
        }

        None
    }

    /// Terminal/iTerm/Warp/Alacritty: `{user}@{host}: {path}`
    fn parse_terminal(&self, lower_app: &str, title: &str) -> Option<ParsedContent> {
        let is_terminal = lower_app.contains("terminal")
            || lower_app.contains("iterm")
            || lower_app.contains("warp")
            || lower_app.contains("alacritty")
            || lower_app.contains("kitty")
            || lower_app.contains("hyper")
            || lower_app.contains("konsole");

        if !is_terminal {
            return None;
        }

        // Format: "{user}@{host}: {path}" or just "{path}"
        if let Some(colon_idx) = title.find(": ") {
            let path = title[colon_idx + 2..].trim();
            if !path.is_empty() {
                return Some(ParsedContent {
                    content_label: path.to_string(),
                    content_type: ContentType::File,
                    confidence: 0.85,
                });
            }
        }

        // Fallback: use entire title
        if !title.is_empty() {
            return Some(ParsedContent {
                content_label: title.to_string(),
                content_type: ContentType::File,
                confidence: 0.60,
            });
        }

        None
    }

    /// Excel/Sheets/Word/Notion: various document formats
    fn parse_document(&self, lower_app: &str, title: &str) -> Option<ParsedContent> {
        let is_office = lower_app.contains("excel")
            || lower_app.contains("word")
            || lower_app.contains("powerpoint")
            || lower_app.contains("numbers")
            || lower_app.contains("pages")
            || lower_app.contains("keynote")
            || lower_app.contains("sheets")
            || lower_app.contains("docs");

        let is_notion = lower_app.contains("notion");

        if is_office {
            // Format: "{filename} - {app_name}"
            if let Some(idx) = title.find(" - ") {
                let file = title[..idx].trim();
                if !file.is_empty() {
                    return Some(ParsedContent {
                        content_label: file.to_string(),
                        content_type: ContentType::File,
                        confidence: 0.90,
                    });
                }
            }
        }

        if is_notion {
            // Notion: "{page} / {workspace}" or "{page} - Notion"
            if let Some(idx) = title.find(" / ") {
                let page = title[..idx].trim();
                if !page.is_empty() {
                    return Some(ParsedContent {
                        content_label: page.to_string(),
                        content_type: ContentType::WebPage,
                        confidence: 0.90,
                    });
                }
            }
            if let Some(idx) = title.find(" - ") {
                let page = title[..idx].trim();
                if !page.is_empty() {
                    return Some(ParsedContent {
                        content_label: page.to_string(),
                        content_type: ContentType::WebPage,
                        confidence: 0.85,
                    });
                }
            }
        }

        None
    }

    /// Figma: `{file} \u{2013} Figma`
    fn parse_design(&self, lower_app: &str, title: &str) -> Option<ParsedContent> {
        let is_figma = lower_app.contains("figma");
        let is_sketch = lower_app.contains("sketch");

        if is_figma {
            // Figma: "{file} \u{2013} Figma" (em-dash) or "{file} - Figma"
            if let Some(idx) = title.find('\u{2013}') {
                let file = title[..idx].trim();
                if !file.is_empty() {
                    return Some(ParsedContent {
                        content_label: file.to_string(),
                        content_type: ContentType::File,
                        confidence: 0.90,
                    });
                }
            }
            if let Some(idx) = title.find(" - ") {
                let file = title[..idx].trim();
                if !file.is_empty() {
                    return Some(ParsedContent {
                        content_label: file.to_string(),
                        content_type: ContentType::File,
                        confidence: 0.85,
                    });
                }
            }
        }

        if is_sketch {
            if let Some(idx) = title.find(" - ") {
                let file = title[..idx].trim();
                if !file.is_empty() {
                    return Some(ParsedContent {
                        content_label: file.to_string(),
                        content_type: ContentType::File,
                        confidence: 0.85,
                    });
                }
            }
        }

        None
    }

    /// Generic fallback: use first segment before " - " as content.
    fn parse_generic(&self, title: &str) -> Option<ParsedContent> {
        if title.is_empty() {
            return None;
        }

        let label = if let Some(idx) = title.find(" - ") {
            title[..idx].trim()
        } else {
            title.trim()
        };

        if label.is_empty() {
            return None;
        }

        Some(ParsedContent {
            content_label: label.to_string(),
            content_type: ContentType::Unknown,
            confidence: 0.50,
        })
    }
}

impl Default for TitleBarParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vscode_file_extraction() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse(
                "Visual Studio Code",
                "main.rs - oneshim-core - Visual Studio Code",
            )
            .unwrap();
        assert_eq!(result.content_label, "main.rs");
        assert_eq!(result.content_type, ContentType::File);
        assert!(result.confidence > 0.9);
    }

    #[test]
    fn cursor_file_extraction() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Cursor", "lib.rs - my-project - Cursor")
            .unwrap();
        assert_eq!(result.content_label, "lib.rs");
        assert_eq!(result.content_type, ContentType::File);
    }

    #[test]
    fn chrome_page_extraction() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Google Chrome", "Rust Programming Language - Google Chrome")
            .unwrap();
        assert_eq!(result.content_label, "Rust Programming Language");
        assert_eq!(result.content_type, ContentType::WebPage);
    }

    #[test]
    fn firefox_page_extraction() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Firefox", "GitHub - My Repo - Mozilla Firefox")
            .unwrap();
        assert_eq!(result.content_label, "GitHub - My Repo");
        assert_eq!(result.content_type, ContentType::WebPage);
    }

    #[test]
    fn arc_page_extraction() {
        let parser = TitleBarParser::new();
        let result = parser.parse("Arc", "Stack Overflow - Arc").unwrap();
        assert_eq!(result.content_label, "Stack Overflow");
        assert_eq!(result.content_type, ContentType::WebPage);
    }

    #[test]
    fn slack_channel_pipe_format() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Slack", "#general | ONESHIM Workspace")
            .unwrap();
        assert_eq!(result.content_label, "#general");
        assert_eq!(result.content_type, ContentType::Channel);
    }

    #[test]
    fn slack_channel_dash_format() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Slack", "ONESHIM Workspace - #engineering")
            .unwrap();
        assert_eq!(result.content_label, "#engineering");
        assert_eq!(result.content_type, ContentType::Channel);
    }

    #[test]
    fn terminal_path_extraction() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("iTerm2", "user@host: ~/projects/oneshim")
            .unwrap();
        assert_eq!(result.content_label, "~/projects/oneshim");
        assert_eq!(result.content_type, ContentType::File);
    }

    #[test]
    fn warp_terminal() {
        let parser = TitleBarParser::new();
        let result = parser.parse("Warp", "dev@server: /var/log").unwrap();
        assert_eq!(result.content_label, "/var/log");
        assert_eq!(result.content_type, ContentType::File);
    }

    #[test]
    fn intellij_em_dash_format() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("IntelliJ IDEA", "my-project \u{2013} Main.java")
            .unwrap();
        assert_eq!(result.content_label, "Main.java");
        assert_eq!(result.content_type, ContentType::File);
    }

    #[test]
    fn intellij_dash_fallback_three_parts() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("IntelliJ IDEA", "myproject - Main.java - IntelliJ IDEA")
            .unwrap();
        assert_eq!(result.content_label, "Main.java");
        assert_eq!(result.content_type, ContentType::File);
        assert!((result.confidence - 0.70).abs() < f32::EPSILON);
    }

    #[test]
    fn intellij_dash_fallback_two_parts() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("IntelliJ IDEA", "myproject - Main.java")
            .unwrap();
        assert_eq!(result.content_label, "myproject");
        assert_eq!(result.content_type, ContentType::File);
        assert!((result.confidence - 0.60).abs() < f32::EPSILON);
    }

    #[test]
    fn excel_file_extraction() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Microsoft Excel", "Budget_2026.xlsx - Excel")
            .unwrap();
        assert_eq!(result.content_label, "Budget_2026.xlsx");
        assert_eq!(result.content_type, ContentType::File);
    }

    #[test]
    fn figma_em_dash_extraction() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Figma", "Design System \u{2013} Figma")
            .unwrap();
        assert_eq!(result.content_label, "Design System");
        assert_eq!(result.content_type, ContentType::File);
    }

    #[test]
    fn notion_page_extraction() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Notion", "Sprint Planning / Team Workspace")
            .unwrap();
        assert_eq!(result.content_label, "Sprint Planning");
        assert_eq!(result.content_type, ContentType::WebPage);
    }

    #[test]
    fn container_detection_rdp() {
        let parser = TitleBarParser::new();
        assert_eq!(
            parser.is_container_app("Microsoft Remote Desktop"),
            Some(ContainerType::Rdp)
        );
    }

    #[test]
    fn container_detection_vm() {
        let parser = TitleBarParser::new();
        assert_eq!(
            parser.is_container_app("Parallels Desktop"),
            Some(ContainerType::Vm)
        );
        assert_eq!(
            parser.is_container_app("VMware Fusion"),
            Some(ContainerType::Vm)
        );
        assert_eq!(parser.is_container_app("UTM"), Some(ContainerType::Vm));
    }

    #[test]
    fn container_detection_vnc() {
        let parser = TitleBarParser::new();
        assert_eq!(
            parser.is_container_app("VNC Viewer"),
            Some(ContainerType::Vnc)
        );
    }

    #[test]
    fn container_detection_citrix() {
        let parser = TitleBarParser::new();
        assert_eq!(
            parser.is_container_app("Citrix Workspace"),
            Some(ContainerType::Citrix)
        );
    }

    #[test]
    fn container_detection_unknown() {
        let parser = TitleBarParser::new();
        assert_eq!(parser.is_container_app("Visual Studio Code"), None);
    }

    #[test]
    fn unknown_app_generic_parse() {
        let parser = TitleBarParser::new();
        let result = parser.parse("SomeApp", "Document Title - SomeApp").unwrap();
        assert_eq!(result.content_label, "Document Title");
        assert_eq!(result.content_type, ContentType::Unknown);
        assert!(result.confidence <= 0.50);
    }

    #[test]
    fn empty_title_returns_none() {
        let parser = TitleBarParser::new();
        assert!(parser.parse("VSCode", "").is_none());
    }

    #[test]
    fn title_with_no_separator() {
        let parser = TitleBarParser::new();
        let result = parser.parse("UnknownApp", "Just a title").unwrap();
        assert_eq!(result.content_label, "Just a title");
        assert_eq!(result.content_type, ContentType::Unknown);
    }

    #[test]
    fn discord_channel_extraction() {
        let parser = TitleBarParser::new();
        let result = parser.parse("Discord", "#dev-chat | My Server").unwrap();
        assert_eq!(result.content_label, "#dev-chat");
        assert_eq!(result.content_type, ContentType::Channel);
    }
}
