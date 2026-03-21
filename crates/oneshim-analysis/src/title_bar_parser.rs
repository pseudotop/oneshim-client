use oneshim_core::models::tiered_memory::{ContainerType, ContentType};

/// Extracted content information from a window title bar.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedContent {
    pub content_label: String,
    pub content_type: ContentType,
    pub confidence: f32,
    /// Project or repository name extracted from IDE/terminal title bars.
    /// e.g., VS Code "main.rs - oneshim-client - Visual Studio Code" → "oneshim-client"
    pub project: Option<String>,
    /// Domain hint extracted from browser page titles via heuristic prefix matching.
    /// e.g., "Gmail - Inbox (3) - Google Chrome" → "gmail"
    pub domain_hint: Option<String>,
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
    /// Xcode: `{project} — {file}` (em-dash \u{2014})
    fn parse_ide(&self, lower_app: &str, title: &str) -> Option<ParsedContent> {
        let is_xcode = lower_app.contains("xcode");
        // "xcode" contains "code", so exclude it from VSCode detection
        let is_vscode = !is_xcode && (lower_app.contains("code") || lower_app.contains("cursor"));
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
                // Extract project from middle segment (index 1) when 3+ parts
                let project = if parts.len() >= 3 {
                    let proj = parts[1].trim();
                    if !proj.is_empty() {
                        Some(proj.to_string())
                    } else {
                        None
                    }
                } else {
                    None
                };
                if !file.is_empty() {
                    return Some(ParsedContent {
                        content_label: file.to_string(),
                        content_type: ContentType::File,
                        confidence: 0.95,
                        project,
                        domain_hint: None,
                    });
                }
            }
        }

        if is_jetbrains {
            // Format: "{project} – {file}" (em-dash \u{2013})
            if let Some(idx) = title.find('\u{2013}') {
                let project_part = title[..idx].trim();
                let file = title[idx + '\u{2013}'.len_utf8()..].trim();
                if !file.is_empty() {
                    let project = if !project_part.is_empty() {
                        Some(project_part.to_string())
                    } else {
                        None
                    };
                    return Some(ParsedContent {
                        content_label: file.to_string(),
                        content_type: ContentType::File,
                        confidence: 0.90,
                        project,
                        domain_hint: None,
                    });
                }
            }
            // Fallback: some JetBrains IDEs use " - " instead
            // Format: "{project} - {file} - {IDE}" or "{project} - {file}"
            let parts: Vec<&str> = title.split(" - ").collect();
            if parts.len() >= 3 {
                // {project} - {file} - {IDE}
                let project_part = parts[0].trim();
                let file = parts[1].trim();
                if !file.is_empty() {
                    let project = if !project_part.is_empty() {
                        Some(project_part.to_string())
                    } else {
                        None
                    };
                    return Some(ParsedContent {
                        content_label: file.to_string(),
                        content_type: ContentType::File,
                        confidence: 0.70,
                        project,
                        domain_hint: None,
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
                        project: None,
                        domain_hint: None,
                    });
                }
            }
        }

        if is_xcode {
            // Format: "{project} — {file}" (em-dash \u{2014})
            if let Some(idx) = title.find('\u{2014}') {
                let project_part = title[..idx].trim();
                let file = title[idx + '\u{2014}'.len_utf8()..].trim();
                if !file.is_empty() {
                    let project = if !project_part.is_empty() {
                        Some(project_part.to_string())
                    } else {
                        None
                    };
                    return Some(ParsedContent {
                        content_label: file.to_string(),
                        content_type: ContentType::File,
                        confidence: 0.90,
                        project,
                        domain_hint: None,
                    });
                }
            }
            // Fallback: Xcode sometimes uses " \u{2013} " (en-dash)
            if let Some(idx) = title.find('\u{2013}') {
                let project_part = title[..idx].trim();
                let file = title[idx + '\u{2013}'.len_utf8()..].trim();
                if !file.is_empty() {
                    let project = if !project_part.is_empty() {
                        Some(project_part.to_string())
                    } else {
                        None
                    };
                    return Some(ParsedContent {
                        content_label: file.to_string(),
                        content_type: ContentType::File,
                        confidence: 0.85,
                        project,
                        domain_hint: None,
                    });
                }
            }
            // Fallback: " - " separator
            let parts: Vec<&str> = title.split(" - ").collect();
            if parts.len() >= 2 {
                let project_part = parts[0].trim();
                let file = parts[1].trim();
                if !file.is_empty() {
                    let project = if !project_part.is_empty() {
                        Some(project_part.to_string())
                    } else {
                        None
                    };
                    return Some(ParsedContent {
                        content_label: file.to_string(),
                        content_type: ContentType::File,
                        confidence: 0.75,
                        project,
                        domain_hint: None,
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
                let domain_hint = Self::extract_domain_hint(page);
                return Some(ParsedContent {
                    content_label: page.to_string(),
                    content_type: ContentType::WebPage,
                    confidence: 0.90,
                    project: None,
                    domain_hint,
                });
            }
        }

        // No separator — use entire title
        let domain_hint = Self::extract_domain_hint(title);
        Some(ParsedContent {
            content_label: title.to_string(),
            content_type: ContentType::WebPage,
            confidence: 0.70,
            project: None,
            domain_hint,
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
                        project: None,
                        domain_hint: None,
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
                        project: None,
                        domain_hint: None,
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
                        project: None,
                        domain_hint: None,
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
                        project: None,
                        domain_hint: None,
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
                let project = Self::extract_project_from_path(path);
                return Some(ParsedContent {
                    content_label: path.to_string(),
                    content_type: ContentType::File,
                    confidence: 0.85,
                    project,
                    domain_hint: None,
                });
            }
        }

        // Fallback: use entire title
        if !title.is_empty() {
            let project = Self::extract_project_from_path(title);
            return Some(ParsedContent {
                content_label: title.to_string(),
                content_type: ContentType::File,
                confidence: 0.60,
                project,
                domain_hint: None,
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
                        project: None,
                        domain_hint: None,
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
                        project: None,
                        domain_hint: None,
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
                        project: None,
                        domain_hint: None,
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
                        project: None,
                        domain_hint: None,
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
                        project: None,
                        domain_hint: None,
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
                        project: None,
                        domain_hint: None,
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
            project: None,
            domain_hint: None,
        })
    }

    /// Extract a project name from a terminal CWD path.
    ///
    /// Takes the last path component (directory name) as the project name.
    /// e.g., "~/projects/oneshim-client" → "oneshim-client"
    ///       "/home/user/code/my-app"    → "my-app"
    fn extract_project_from_path(path: &str) -> Option<String> {
        let trimmed = path.trim().trim_end_matches('/');
        if trimmed.is_empty() || trimmed == "~" || trimmed == "/" {
            return None;
        }

        // Take the last path component
        let last = trimmed.rsplit('/').next().unwrap_or(trimmed);
        if last.is_empty() || last == "~" {
            return None;
        }

        Some(last.to_string())
    }

    /// Extract a domain hint from a browser page title using known prefix patterns.
    ///
    /// Maps common page title prefixes to normalized domain identifiers.
    /// This is heuristic-based and aims for ~80% accuracy on common sites.
    fn extract_domain_hint(page_title: &str) -> Option<String> {
        /// Known title prefix → domain hint mappings.
        /// Checked in order; first match wins.
        const DOMAIN_PATTERNS: &[(&str, &str)] = &[
            // Google services
            ("Gmail", "gmail"),
            ("Google Calendar", "google-calendar"),
            ("Google Meet", "google-meet"),
            ("Google Drive", "google-drive"),
            ("Google Docs", "google-docs"),
            ("Google Sheets", "google-sheets"),
            ("Google Slides", "google-slides"),
            // Developer tools
            ("GitHub", "github"),
            ("GitLab", "gitlab"),
            ("Bitbucket", "bitbucket"),
            ("Stack Overflow", "stackoverflow"),
            ("Stack Exchange", "stackexchange"),
            ("npm", "npm"),
            ("crates.io", "crates-io"),
            ("PyPI", "pypi"),
            ("Docker Hub", "dockerhub"),
            ("Rust Playground", "rust-playground"),
            // Project management
            ("Jira", "jira"),
            ("Confluence", "confluence"),
            ("Trello", "trello"),
            ("Asana", "asana"),
            ("Linear", "linear"),
            ("Notion", "notion"),
            // Communication
            ("Slack", "slack"),
            ("Discord", "discord"),
            ("Microsoft Teams", "teams"),
            ("Zoom", "zoom"),
            // Documentation / Knowledge
            ("Wikipedia", "wikipedia"),
            ("MDN Web Docs", "mdn"),
            ("docs.rs", "docs-rs"),
            ("Hacker News", "hackernews"),
            ("Reddit", "reddit"),
            ("Medium", "medium"),
            ("Dev.to", "devto"),
            // Cloud / Infra
            ("AWS", "aws"),
            ("Azure", "azure"),
            ("Google Cloud", "gcp"),
            ("Vercel", "vercel"),
            ("Netlify", "netlify"),
            ("Cloudflare", "cloudflare"),
            // Design
            ("Figma", "figma"),
            // AI
            ("ChatGPT", "chatgpt"),
            ("Claude", "claude"),
            // Office
            ("Outlook", "outlook"),
            ("OneDrive", "onedrive"),
            ("Dropbox", "dropbox"),
            // Video / Media
            ("YouTube", "youtube"),
            ("Twitch", "twitch"),
            ("Spotify", "spotify"),
        ];

        let trimmed = page_title.trim();
        for &(prefix, domain) in DOMAIN_PATTERNS {
            if trimmed.starts_with(prefix) {
                return Some(domain.to_string());
            }
        }

        None
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

    // ── Existing content extraction tests ──────────────────────────────

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

    // ── Task 1: Project detection from IDE title bars ──────────────────

    #[test]
    fn vscode_project_extraction() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse(
                "Visual Studio Code",
                "main.rs - oneshim-client - Visual Studio Code",
            )
            .unwrap();
        assert_eq!(result.content_label, "main.rs");
        assert_eq!(result.project, Some("oneshim-client".to_string()));
    }

    #[test]
    fn cursor_project_extraction() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Cursor", "lib.rs - my-project - Cursor")
            .unwrap();
        assert_eq!(result.content_label, "lib.rs");
        assert_eq!(result.project, Some("my-project".to_string()));
    }

    #[test]
    fn vscode_two_parts_no_project() {
        // Only file + app name, no project in the middle
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Visual Studio Code", "Welcome - Visual Studio Code")
            .unwrap();
        assert_eq!(result.content_label, "Welcome");
        assert_eq!(result.project, None);
    }

    #[test]
    fn jetbrains_em_dash_project_extraction() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("IntelliJ IDEA", "oneshim-client \u{2013} Main.java")
            .unwrap();
        assert_eq!(result.content_label, "Main.java");
        assert_eq!(result.project, Some("oneshim-client".to_string()));
    }

    #[test]
    fn jetbrains_dash_three_parts_project() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("IntelliJ IDEA", "myproject - Main.java - IntelliJ IDEA")
            .unwrap();
        assert_eq!(result.content_label, "Main.java");
        assert_eq!(result.project, Some("myproject".to_string()));
    }

    #[test]
    fn pycharm_project_extraction() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("PyCharm", "backend-api \u{2013} settings.py")
            .unwrap();
        assert_eq!(result.content_label, "settings.py");
        assert_eq!(result.project, Some("backend-api".to_string()));
    }

    #[test]
    fn rustrover_project_extraction() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("RustRover", "oneshim-client \u{2013} lib.rs")
            .unwrap();
        assert_eq!(result.content_label, "lib.rs");
        assert_eq!(result.project, Some("oneshim-client".to_string()));
    }

    #[test]
    fn xcode_em_dash_project_extraction() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Xcode", "oneshim-client \u{2014} main.swift")
            .unwrap();
        assert_eq!(result.content_label, "main.swift");
        assert_eq!(result.content_type, ContentType::File);
        assert_eq!(result.project, Some("oneshim-client".to_string()));
        assert!((result.confidence - 0.90).abs() < f32::EPSILON);
    }

    #[test]
    fn xcode_en_dash_project_extraction() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Xcode", "MyApp \u{2013} ViewController.swift")
            .unwrap();
        assert_eq!(result.content_label, "ViewController.swift");
        assert_eq!(result.project, Some("MyApp".to_string()));
    }

    #[test]
    fn xcode_dash_fallback() {
        let parser = TitleBarParser::new();
        let result = parser.parse("Xcode", "MyApp - AppDelegate.swift").unwrap();
        assert_eq!(result.content_label, "AppDelegate.swift");
        assert_eq!(result.project, Some("MyApp".to_string()));
    }

    // ── Task 1: Project detection from terminal CWD paths ──────────────

    #[test]
    fn terminal_project_from_cwd_path() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("iTerm2", "user@host: ~/projects/oneshim-client")
            .unwrap();
        assert_eq!(result.content_label, "~/projects/oneshim-client");
        assert_eq!(result.project, Some("oneshim-client".to_string()));
    }

    #[test]
    fn terminal_project_from_absolute_path() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Terminal", "user@host: /home/dev/code/my-app")
            .unwrap();
        assert_eq!(result.project, Some("my-app".to_string()));
    }

    #[test]
    fn terminal_home_dir_no_project() {
        let parser = TitleBarParser::new();
        let result = parser.parse("Terminal", "user@host: ~").unwrap();
        assert_eq!(result.content_label, "~");
        assert_eq!(result.project, None);
    }

    #[test]
    fn terminal_root_dir_no_project() {
        let parser = TitleBarParser::new();
        let result = parser.parse("Alacritty", "root@host: /").unwrap();
        assert_eq!(result.content_label, "/");
        assert_eq!(result.project, None);
    }

    #[test]
    fn terminal_trailing_slash_stripped() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Kitty", "user@host: ~/projects/oneshim/")
            .unwrap();
        assert_eq!(result.project, Some("oneshim".to_string()));
    }

    // ── Task 1: Non-IDE apps have no project field ─────────────────────

    #[test]
    fn browser_no_project() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Google Chrome", "Rust Programming Language - Google Chrome")
            .unwrap();
        assert_eq!(result.project, None);
    }

    #[test]
    fn communication_no_project() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Slack", "#general | ONESHIM Workspace")
            .unwrap();
        assert_eq!(result.project, None);
    }

    #[test]
    fn generic_no_project() {
        let parser = TitleBarParser::new();
        let result = parser.parse("SomeApp", "Document Title - SomeApp").unwrap();
        assert_eq!(result.project, None);
    }

    // ── Task 2: URL domain extraction from browser titles ──────────────

    #[test]
    fn browser_domain_hint_gmail() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Google Chrome", "Gmail - Inbox (3) - Google Chrome")
            .unwrap();
        assert_eq!(result.domain_hint, Some("gmail".to_string()));
    }

    #[test]
    fn browser_domain_hint_github() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse(
                "Google Chrome",
                "GitHub - pseudotop/oneshim-client - Google Chrome",
            )
            .unwrap();
        assert_eq!(result.content_label, "GitHub - pseudotop/oneshim-client");
        assert_eq!(result.domain_hint, Some("github".to_string()));
    }

    #[test]
    fn browser_domain_hint_stackoverflow() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Arc", "Stack Overflow - How to parse titles - Arc")
            .unwrap();
        assert_eq!(result.domain_hint, Some("stackoverflow".to_string()));
    }

    #[test]
    fn browser_domain_hint_jira() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Firefox", "Jira - PROJ-123 Sprint Board - Mozilla Firefox")
            .unwrap();
        assert_eq!(result.domain_hint, Some("jira".to_string()));
    }

    #[test]
    fn browser_domain_hint_youtube() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Google Chrome", "YouTube - Rust Tutorial - Google Chrome")
            .unwrap();
        assert_eq!(result.domain_hint, Some("youtube".to_string()));
    }

    #[test]
    fn browser_domain_hint_reddit() {
        let parser = TitleBarParser::new();
        let result = parser.parse("Safari", "Reddit - r/rust - Safari").unwrap();
        assert_eq!(result.domain_hint, Some("reddit".to_string()));
    }

    #[test]
    fn browser_domain_hint_aws() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Brave", "AWS Management Console - Brave")
            .unwrap();
        assert_eq!(result.domain_hint, Some("aws".to_string()));
    }

    #[test]
    fn browser_domain_hint_google_docs() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Google Chrome", "Google Docs - My Document - Google Chrome")
            .unwrap();
        assert_eq!(result.domain_hint, Some("google-docs".to_string()));
    }

    #[test]
    fn browser_domain_hint_notion() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Google Chrome", "Notion - Sprint Planning - Google Chrome")
            .unwrap();
        assert_eq!(result.domain_hint, Some("notion".to_string()));
    }

    #[test]
    fn browser_domain_hint_chatgpt() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Google Chrome", "ChatGPT - Google Chrome")
            .unwrap();
        assert_eq!(result.domain_hint, Some("chatgpt".to_string()));
    }

    #[test]
    fn browser_domain_hint_claude() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Google Chrome", "Claude - New Conversation - Google Chrome")
            .unwrap();
        assert_eq!(result.domain_hint, Some("claude".to_string()));
    }

    #[test]
    fn browser_no_domain_hint_unknown_site() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Google Chrome", "My Personal Blog - Google Chrome")
            .unwrap();
        assert_eq!(result.domain_hint, None);
    }

    #[test]
    fn browser_domain_hint_no_separator() {
        // Browser title without " - " separator
        let parser = TitleBarParser::new();
        let result = parser.parse("Google Chrome", "Gmail").unwrap();
        assert_eq!(result.domain_hint, Some("gmail".to_string()));
    }

    #[test]
    fn browser_domain_hint_mdn() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Firefox", "MDN Web Docs - CSS Grid - Mozilla Firefox")
            .unwrap();
        assert_eq!(result.domain_hint, Some("mdn".to_string()));
    }

    #[test]
    fn browser_domain_hint_docs_rs() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Google Chrome", "docs.rs - tokio - Google Chrome")
            .unwrap();
        assert_eq!(result.domain_hint, Some("docs-rs".to_string()));
    }

    #[test]
    fn browser_domain_hint_linear() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("Google Chrome", "Linear - Issue ONS-42 - Google Chrome")
            .unwrap();
        assert_eq!(result.domain_hint, Some("linear".to_string()));
    }

    // ── Task 2: Non-browser apps have no domain_hint ───────────────────

    #[test]
    fn ide_no_domain_hint() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse(
                "Visual Studio Code",
                "main.rs - oneshim-client - Visual Studio Code",
            )
            .unwrap();
        assert_eq!(result.domain_hint, None);
    }

    #[test]
    fn terminal_no_domain_hint() {
        let parser = TitleBarParser::new();
        let result = parser
            .parse("iTerm2", "user@host: ~/projects/oneshim")
            .unwrap();
        assert_eq!(result.domain_hint, None);
    }

    // ── Helper function tests ──────────────────────────────────────────

    #[test]
    fn extract_project_from_path_basic() {
        assert_eq!(
            TitleBarParser::extract_project_from_path("~/projects/my-app"),
            Some("my-app".to_string())
        );
    }

    #[test]
    fn extract_project_from_path_absolute() {
        assert_eq!(
            TitleBarParser::extract_project_from_path("/home/user/code/backend"),
            Some("backend".to_string())
        );
    }

    #[test]
    fn extract_project_from_path_home_only() {
        assert_eq!(TitleBarParser::extract_project_from_path("~"), None);
    }

    #[test]
    fn extract_project_from_path_root_only() {
        assert_eq!(TitleBarParser::extract_project_from_path("/"), None);
    }

    #[test]
    fn extract_project_from_path_trailing_slash() {
        assert_eq!(
            TitleBarParser::extract_project_from_path("~/projects/app/"),
            Some("app".to_string())
        );
    }

    #[test]
    fn extract_domain_hint_known_prefix() {
        assert_eq!(
            TitleBarParser::extract_domain_hint("GitHub - pseudotop/repo"),
            Some("github".to_string())
        );
    }

    #[test]
    fn extract_domain_hint_unknown_prefix() {
        assert_eq!(
            TitleBarParser::extract_domain_hint("Some Random Site - Page"),
            None
        );
    }

    #[test]
    fn extract_domain_hint_google_calendar() {
        assert_eq!(
            TitleBarParser::extract_domain_hint("Google Calendar - March 2026"),
            Some("google-calendar".to_string())
        );
    }

    #[test]
    fn extract_domain_hint_npm() {
        assert_eq!(
            TitleBarParser::extract_domain_hint("npm | react"),
            Some("npm".to_string())
        );
    }

    #[test]
    fn extract_domain_hint_hacker_news() {
        assert_eq!(
            TitleBarParser::extract_domain_hint("Hacker News"),
            Some("hackernews".to_string())
        );
    }
}
