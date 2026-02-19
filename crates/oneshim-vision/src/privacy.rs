//! 프라이버시 필터.
//!
//! 창 제목 등에서 PII 새니타이징.
//! PII 필터 레벨별 (Off, Basic, Standard, Strict) 단계적 마스킹 지원.
//! 민감 앱 자동 감지 (비밀번호 관리자, 금융, 보안 앱).

use oneshim_core::config::PiiFilterLevel;

// ============================================================
// 레벨 기반 PII 필터
// ============================================================

/// PII 레벨에 따라 창 제목 새니타이징
pub fn sanitize_title_with_level(title: &str, level: PiiFilterLevel) -> String {
    match level {
        PiiFilterLevel::Off => title.to_string(),
        PiiFilterLevel::Basic => {
            let mut result = title.to_string();
            result = mask_emails(&result);
            result = mask_phone_numbers(&result);
            result
        }
        PiiFilterLevel::Standard => {
            let mut result = sanitize_title_with_level(title, PiiFilterLevel::Basic);
            result = mask_credit_cards(&result);
            result = mask_korean_id(&result);
            result = mask_user_paths(&result);
            result
        }
        PiiFilterLevel::Strict => {
            let mut result = sanitize_title_with_level(title, PiiFilterLevel::Standard);
            result = mask_api_keys(&result);
            result = mask_ip_addresses(&result);
            result
        }
    }
}

/// 기존 API 호환: Standard 레벨로 새니타이징
pub fn sanitize_title(title: &str) -> String {
    sanitize_title_with_level(title, PiiFilterLevel::Standard)
}

// ============================================================
// 민감 앱 자동 감지
// ============================================================

/// 민감 앱 키워드 목록 (비밀번호 관리자, 금융, 보안, 메신저)
pub const SENSITIVE_APP_KEYWORDS: &[&str] = &[
    // 비밀번호 관리자
    "1password",
    "lastpass",
    "bitwarden",
    "dashlane",
    "keepass",
    "enpass",
    "nordpass",
    // 금융
    "bank",
    "banking",
    "wallet",
    "trading",
    "crypto",
    "coinbase",
    "binance",
    // 보안
    "authenticator",
    "2fa",
    "otp",
    "vault",
    "keychain",
    // 메신저 (선택적)
    "signal",
];

/// 앱 이름이 민감 앱인지 확인
pub fn is_sensitive_app(app_name: &str) -> bool {
    let lower = app_name.to_lowercase();
    SENSITIVE_APP_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

/// 앱 이름이 제외 목록 패턴에 매칭되는지 확인 (glob 스타일: *bank* 등)
pub fn matches_exclusion_pattern(app_name: &str, patterns: &[String]) -> bool {
    let lower = app_name.to_lowercase();
    patterns.iter().any(|pattern| {
        let pat = pattern.to_lowercase();
        if let Some(rest) = pat.strip_prefix('*') {
            if let Some(keyword) = rest.strip_suffix('*') {
                // *keyword* → contains
                lower.contains(keyword)
            } else {
                // *suffix → ends_with
                lower.ends_with(rest)
            }
        } else if let Some(prefix) = pat.strip_suffix('*') {
            // prefix* → starts_with
            lower.starts_with(prefix)
        } else {
            // 정확한 매칭
            lower == pat
        }
    })
}

/// 앱/제목이 프라이버시 정책에 의해 제외되어야 하는지 종합 판단
pub fn should_exclude(
    app_name: &str,
    window_title: &str,
    excluded_apps: &[String],
    excluded_app_patterns: &[String],
    excluded_title_patterns: &[String],
    auto_exclude_sensitive: bool,
) -> bool {
    // 1. 정확한 앱 이름 매칭
    let lower = app_name.to_lowercase();
    if excluded_apps.iter().any(|a| a.to_lowercase() == lower) {
        return true;
    }

    // 2. 앱 이름 패턴 매칭
    if matches_exclusion_pattern(app_name, excluded_app_patterns) {
        return true;
    }

    // 3. 창 제목 패턴 매칭
    if matches_exclusion_pattern(window_title, excluded_title_patterns) {
        return true;
    }

    // 4. 민감 앱 자동 감지
    if auto_exclude_sensitive && is_sensitive_app(app_name) {
        return true;
    }

    false
}

// ============================================================
// PII 마스킹 함수
// ============================================================

/// 이메일 주소 마스킹 (간단한 패턴)
fn mask_emails(text: &str) -> String {
    let mut result = String::new();
    let mut i = 0;
    let chars: Vec<char> = text.chars().collect();

    while i < chars.len() {
        if let Some(at_pos) = chars[i..].iter().position(|&c| c == '@') {
            let at_abs = i + at_pos;
            // @ 앞의 단어 찾기
            let start = chars[..at_abs]
                .iter()
                .rposition(|c| c.is_whitespace() || *c == '<' || *c == '(')
                .map(|p| p + 1)
                .unwrap_or(i);

            // @ 뒤의 도메인 찾기
            let end = chars[at_abs + 1..]
                .iter()
                .position(|c| c.is_whitespace() || *c == '>' || *c == ')')
                .map(|p| at_abs + 1 + p)
                .unwrap_or(chars.len());

            if at_abs > start && end > at_abs + 1 {
                result.extend(&chars[i..start]);
                result.push_str("[EMAIL]");
                i = end;
                continue;
            }
        }

        if i < chars.len() {
            result.push(chars[i]);
            i += 1;
        } else {
            break;
        }
    }

    result
}

/// 전화번호 마스킹 (한국 + 국제 형식)
fn mask_phone_numbers(text: &str) -> String {
    let mut result = text.to_string();
    let chars: Vec<char> = result.chars().collect();
    let len = chars.len();

    // 역방향 스캔으로 전화번호 패턴 찾기
    // 한국: 010-1234-5678, 02-123-4567, +82-10-1234-5678
    // 국제: +1-555-123-4567
    let mut masked = String::new();
    let mut i = 0;
    while i < len {
        // 전화번호 시작 가능 문자: +, 0, 숫자
        if (chars[i] == '+' || chars[i].is_ascii_digit()) && is_phone_number_start(&chars, i, len) {
            if let Some(end) = find_phone_number_end(&chars, i, len) {
                masked.push_str("[PHONE]");
                i = end;
                continue;
            }
        }
        masked.push(chars[i]);
        i += 1;
    }
    result = masked;
    result
}

/// 전화번호 시작 패턴인지 확인
fn is_phone_number_start(chars: &[char], pos: usize, _len: usize) -> bool {
    // + 으로 시작 (국제번호)
    if chars[pos] == '+' {
        return true;
    }
    // 0으로 시작하는 한국 전화번호 (010, 02, 031 등)
    if chars[pos] == '0' {
        return true;
    }
    false
}

/// 전화번호 끝 위치를 반환 (10자리 이상 숫자+구분자)
fn find_phone_number_end(chars: &[char], start: usize, len: usize) -> Option<usize> {
    let mut i = start;
    let mut digit_count = 0;
    let mut separator_count = 0;

    // + 건너뛰기
    if i < len && chars[i] == '+' {
        i += 1;
    }

    while i < len {
        if chars[i].is_ascii_digit() {
            digit_count += 1;
            i += 1;
        } else if chars[i] == '-' || chars[i] == ' ' || chars[i] == '.' {
            separator_count += 1;
            if separator_count > 4 {
                break;
            }
            i += 1;
        } else {
            break;
        }
    }

    // 전화번호: 최소 9자리 숫자, 최소 1개 구분자
    if digit_count >= 9 && separator_count >= 1 {
        Some(i)
    } else {
        None
    }
}

/// 신용카드 패턴 마스킹 (4자리-4자리-4자리-4자리)
fn mask_credit_cards(text: &str) -> String {
    let result = text.to_string();
    // 간단한 4-4-4-4 패턴
    let patterns = [r"\d{4}[- ]\d{4}[- ]\d{4}[- ]\d{4}", r"\d{16}"];
    for _pattern in &patterns {
        // 간단한 구현: 16자리 연속 숫자 마스킹
        if result.chars().filter(|c| c.is_ascii_digit()).count() >= 16 {
            let mut masked = String::new();
            let mut digit_count = 0;
            for ch in result.chars() {
                if ch.is_ascii_digit() {
                    digit_count += 1;
                    if digit_count > 16 {
                        masked.push(ch);
                    }
                } else {
                    if digit_count >= 16 {
                        masked.push_str("[CARD]");
                    }
                    masked.push(ch);
                    digit_count = 0;
                }
            }
            if digit_count >= 16 {
                masked.push_str("[CARD]");
            }
            return masked;
        }
    }
    result
}

/// 한국 주민등록번호 마스킹 (6자리-7자리)
fn mask_korean_id(text: &str) -> String {
    let mut result = text.to_string();
    if result.contains('-') {
        let parts: Vec<String> = result.split('-').map(|s| s.to_string()).collect();
        for window in parts.windows(2) {
            if window[0].len() >= 6
                && window[0].chars().rev().take(6).all(|c| c.is_ascii_digit())
                && window[1].len() >= 7
                && window[1].chars().take(7).all(|c| c.is_ascii_digit())
            {
                let needle = format!("{}-{}", &window[0][window[0].len() - 6..], &window[1][..7]);
                result = result.replace(&needle, "[KR_ID]");
            }
        }
    }
    result
}

/// API 키 패턴 마스킹 (sk-, pk-, api_, key_ 등 접두어 + 영숫자)
fn mask_api_keys(text: &str) -> String {
    let mut result = text.to_string();
    // 일반적인 API 키 접두어 패턴
    let prefixes = [
        "sk-",
        "pk-",
        "sk_",
        "pk_",
        "api_",
        "key_",
        "token_",
        "secret_",
        "AKIA",
        "ghp_",
        "gho_",
        "github_pat_",
        "xoxb-",
        "xoxp-",
    ];

    for prefix in &prefixes {
        while let Some(pos) = result.find(prefix) {
            let start = pos;
            let after = &result[pos + prefix.len()..];
            // 키 끝 찾기: 공백/구두점까지
            let end_offset = after
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',' || c == ';')
                .unwrap_or(after.len());

            // 최소 8자 이상의 키
            if end_offset >= 8 {
                let key_end = pos + prefix.len() + end_offset;
                result = format!("{}[API_KEY]{}", &result[..start], &result[key_end..]);
            } else {
                break;
            }
        }
    }

    result
}

/// IP 주소 마스킹 (IPv4)
fn mask_ip_addresses(text: &str) -> String {
    let mut result = text.to_string();
    let chars: Vec<char> = result.chars().collect();
    let len = chars.len();
    let mut masked = String::new();
    let mut i = 0;

    while i < len {
        if chars[i].is_ascii_digit() {
            // IP 주소 패턴 확인: N.N.N.N (각 N은 1-3자리)
            if let Some((ip_end, is_valid)) = try_parse_ipv4(&chars, i, len) {
                if is_valid {
                    masked.push_str("[IP]");
                    i = ip_end;
                    continue;
                }
            }
        }
        masked.push(chars[i]);
        i += 1;
    }

    result = masked;
    result
}

/// IPv4 주소 파싱 시도 — (끝 위치, 유효 여부) 반환
fn try_parse_ipv4(chars: &[char], start: usize, len: usize) -> Option<(usize, bool)> {
    let mut i = start;
    let mut octet_count = 0;

    for _ in 0..4 {
        let octet_start = i;
        while i < len && chars[i].is_ascii_digit() {
            i += 1;
        }
        let octet_len = i - octet_start;
        if octet_len == 0 || octet_len > 3 {
            return None;
        }

        // 숫자값 확인 (0-255)
        let octet_str: String = chars[octet_start..i].iter().collect();
        if let Ok(val) = octet_str.parse::<u32>() {
            if val > 255 {
                return None;
            }
        }

        octet_count += 1;
        if octet_count < 4 {
            if i < len && chars[i] == '.' {
                i += 1;
            } else {
                return None;
            }
        }
    }

    // 뒤에 숫자가 더 오면 IP 주소가 아님
    if i < len && chars[i].is_ascii_digit() {
        return None;
    }

    Some((i, octet_count == 4))
}

/// 파일 경로에서 사용자명 마스킹
fn mask_user_paths(text: &str) -> String {
    let mut result = text.to_string();

    // macOS: /Users/username/
    if let Some(start) = result.find("/Users/") {
        let after = &result[start + 7..];
        if let Some(end) = after.find('/') {
            let username = &after[..end];
            result = result.replace(&format!("/Users/{username}/"), "/Users/[USER]/");
        }
    }

    // Windows: C:\Users\username\
    if let Some(start) = result.find(r"C:\Users\") {
        let after = &result[start + 9..];
        if let Some(end) = after.find('\\') {
            let username = &after[..end];
            result = result.replace(&format!(r"C:\Users\{username}\"), r"C:\Users\[USER]\");
        }
    }

    // Linux: /home/username/
    if let Some(start) = result.find("/home/") {
        let after = &result[start + 6..];
        if let Some(end) = after.find('/') {
            let username = &after[..end];
            result = result.replace(&format!("/home/{username}/"), "/home/[USER]/");
        }
    }

    result
}

// ============================================================
// 테스트
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- 기존 테스트 (하위 호환) ---

    #[test]
    fn sanitize_email() {
        let result = sanitize_title("Login - user@example.com");
        assert!(result.contains("[EMAIL]"));
        assert!(!result.contains("user@example.com"));
    }

    #[test]
    fn sanitize_macos_path() {
        let result = sanitize_title("File: /Users/johndoe/Documents/secret.txt");
        assert!(result.contains("[USER]"));
        assert!(!result.contains("johndoe"));
    }

    #[test]
    fn sanitize_windows_path() {
        let result = sanitize_title(r"File: C:\Users\johndoe\Documents\secret.txt");
        assert!(result.contains("[USER]"));
    }

    #[test]
    fn no_pii_unchanged() {
        let title = "Visual Studio Code - main.rs";
        assert_eq!(sanitize_title(title), title);
    }

    // --- PII 레벨별 테스트 ---

    #[test]
    fn level_off_no_filter() {
        let title = "user@example.com - 010-1234-5678";
        let result = sanitize_title_with_level(title, PiiFilterLevel::Off);
        assert_eq!(result, title);
    }

    #[test]
    fn level_basic_email_and_phone() {
        let result =
            sanitize_title_with_level("user@example.com 010-1234-5678", PiiFilterLevel::Basic);
        assert!(result.contains("[EMAIL]"));
        assert!(result.contains("[PHONE]"));
    }

    #[test]
    fn level_strict_masks_api_keys() {
        let result =
            sanitize_title_with_level("key: sk-abc123def456ghi789jkl0", PiiFilterLevel::Strict);
        assert!(result.contains("[API_KEY]"));
        assert!(!result.contains("sk-abc123"));
    }

    #[test]
    fn level_strict_masks_ip() {
        let result =
            sanitize_title_with_level("server at 192.168.1.100:8080", PiiFilterLevel::Strict);
        assert!(result.contains("[IP]"));
        assert!(!result.contains("192.168.1.100"));
    }

    // --- 민감 앱 감지 테스트 ---

    #[test]
    fn detect_sensitive_apps() {
        assert!(is_sensitive_app("1Password"));
        assert!(is_sensitive_app("Bitwarden"));
        assert!(is_sensitive_app("KB Banking"));
        assert!(is_sensitive_app("Google Authenticator"));
        assert!(!is_sensitive_app("Visual Studio Code"));
        assert!(!is_sensitive_app("Chrome"));
    }

    #[test]
    fn exclusion_pattern_glob() {
        let patterns = vec!["*bank*".to_string(), "Discord".to_string()];
        assert!(matches_exclusion_pattern("KB Banking", &patterns));
        assert!(matches_exclusion_pattern("Discord", &patterns));
        assert!(!matches_exclusion_pattern("Chrome", &patterns));
    }

    #[test]
    fn should_exclude_comprehensive() {
        assert!(should_exclude(
            "1Password",
            "Unlock",
            &[],
            &[],
            &[],
            true, // auto_exclude_sensitive
        ));
        assert!(!should_exclude("Chrome", "Google", &[], &[], &[], true,));
        // 명시적 앱 제외
        assert!(should_exclude(
            "Slack",
            "General",
            &["Slack".to_string()],
            &[],
            &[],
            false,
        ));
        // 제목 패턴 매칭
        assert!(should_exclude(
            "Chrome",
            "My Password Manager",
            &[],
            &[],
            &["*password*".to_string()],
            false,
        ));
    }

    // --- 전화번호 마스킹 테스트 ---

    #[test]
    fn mask_korean_phone() {
        let result = mask_phone_numbers("call 010-1234-5678 now");
        assert!(result.contains("[PHONE]"));
        assert!(!result.contains("010-1234-5678"));
    }

    #[test]
    fn mask_international_phone() {
        let result = mask_phone_numbers("reach +82-10-1234-5678");
        assert!(result.contains("[PHONE]"));
    }

    // --- IP 주소 마스킹 테스트 ---

    #[test]
    fn mask_ipv4_basic() {
        let result = mask_ip_addresses("connecting to 10.0.0.1 on port 80");
        assert!(result.contains("[IP]"));
        assert!(!result.contains("10.0.0.1"));
    }

    #[test]
    fn mask_ipv4_preserves_non_ip() {
        let result = mask_ip_addresses("version 1.2.3");
        // 3개 옥텟이므로 IP 아님
        assert!(!result.contains("[IP]"));
    }

    // --- Linux 경로 마스킹 테스트 ---

    #[test]
    fn mask_linux_home_path() {
        let result = mask_user_paths("/home/devuser/projects/secret.txt");
        assert!(result.contains("[USER]"));
        assert!(!result.contains("devuser"));
    }
}
