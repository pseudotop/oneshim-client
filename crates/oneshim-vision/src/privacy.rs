use oneshim_core::config::PiiFilterLevel;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PiiMarker {
    Email,
    Phone,
    Card,
    KoreanId,
    ApiKey,
    Ip,
    UserPath,
}

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

pub fn sanitize_title(title: &str) -> String {
    sanitize_title_with_level(title, PiiFilterLevel::Standard)
}

pub fn detect_pii_markers_with_level(text: &str, level: PiiFilterLevel) -> Vec<PiiMarker> {
    let masked = sanitize_title_with_level(text, level);
    let mut markers = Vec::new();

    if marker_inserted(text, &masked, "[EMAIL]") {
        markers.push(PiiMarker::Email);
    }
    if marker_inserted(text, &masked, "[PHONE]") {
        markers.push(PiiMarker::Phone);
    }
    if marker_inserted(text, &masked, "[CARD]") {
        markers.push(PiiMarker::Card);
    }
    if marker_inserted(text, &masked, "[KR_ID]") {
        markers.push(PiiMarker::KoreanId);
    }
    if marker_inserted(text, &masked, "[API_KEY]") {
        markers.push(PiiMarker::ApiKey);
    }
    if marker_inserted(text, &masked, "[IP]") {
        markers.push(PiiMarker::Ip);
    }
    if marker_inserted(text, &masked, "[USER]") {
        markers.push(PiiMarker::UserPath);
    }

    markers
}

pub fn is_sensitive_segment_with_level(text: &str, level: PiiFilterLevel) -> bool {
    !detect_pii_markers_with_level(text, level).is_empty()
}

fn marker_inserted(original: &str, masked: &str, marker: &str) -> bool {
    masked.contains(marker) && (!original.contains(marker) || masked != original)
}

pub const SENSITIVE_APP_KEYWORDS: &[&str] = &[
    "1password",
    "lastpass",
    "bitwarden",
    "dashlane",
    "keepass",
    "enpass",
    "nordpass",
    "bank",
    "banking",
    "wallet",
    "trading",
    "crypto",
    "coinbase",
    "binance",
    "authenticator",
    "2fa",
    "otp",
    "vault",
    "keychain",
    "signal",
];

pub fn is_sensitive_app(app_name: &str) -> bool {
    let lower = app_name.to_lowercase();
    SENSITIVE_APP_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

pub fn matches_exclusion_pattern(app_name: &str, patterns: &[String]) -> bool {
    let lower = app_name.to_lowercase();
    patterns.iter().any(|pattern| {
        let pat = pattern.to_lowercase();
        if let Some(rest) = pat.strip_prefix('*') {
            if let Some(keyword) = rest.strip_suffix('*') {
                lower.contains(keyword)
            } else {
                lower.ends_with(rest)
            }
        } else if let Some(prefix) = pat.strip_suffix('*') {
            lower.starts_with(prefix)
        } else {
            lower == pat
        }
    })
}

pub fn should_exclude(
    app_name: &str,
    window_title: &str,
    excluded_apps: &[String],
    excluded_app_patterns: &[String],
    excluded_title_patterns: &[String],
    auto_exclude_sensitive: bool,
) -> bool {
    let lower = app_name.to_lowercase();
    if excluded_apps.iter().any(|a| a.to_lowercase() == lower) {
        return true;
    }

    if matches_exclusion_pattern(app_name, excluded_app_patterns) {
        return true;
    }

    if matches_exclusion_pattern(window_title, excluded_title_patterns) {
        return true;
    }

    if auto_exclude_sensitive && is_sensitive_app(app_name) {
        return true;
    }

    false
}

fn mask_emails(text: &str) -> String {
    let mut result = String::new();
    let mut i = 0;
    let chars: Vec<char> = text.chars().collect();

    while i < chars.len() {
        if let Some(at_pos) = chars[i..].iter().position(|&c| c == '@') {
            let at_abs = i + at_pos;
            let start = chars[..at_abs]
                .iter()
                .rposition(|c| c.is_whitespace() || *c == '<' || *c == '(')
                .map(|p| p + 1)
                .unwrap_or(i);

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

fn mask_phone_numbers(text: &str) -> String {
    let mut result = text.to_string();
    let chars: Vec<char> = result.chars().collect();
    let len = chars.len();

    let mut masked = String::new();
    let mut i = 0;
    while i < len {
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

fn is_phone_number_start(chars: &[char], pos: usize, _len: usize) -> bool {
    if chars[pos] == '+' {
        return true;
    }
    if chars[pos] == '0' {
        return true;
    }
    false
}

fn find_phone_number_end(chars: &[char], start: usize, len: usize) -> Option<usize> {
    let mut i = start;
    let mut digit_count = 0;
    let mut separator_count = 0;

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

    if digit_count >= 9 && separator_count >= 1 {
        Some(i)
    } else {
        None
    }
}

fn mask_credit_cards(text: &str) -> String {
    let result = text.to_string();
    let patterns = [r"\d{4}[- ]\d{4}[- ]\d{4}[- ]\d{4}", r"\d{16}"];
    for _pattern in &patterns {
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

fn mask_api_keys(text: &str) -> String {
    let mut result = text.to_string();
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
        "ghs_",
        "github_pat_",
        "xoxb-",
        "xoxp-",
    ];

    for prefix in &prefixes {
        while let Some(pos) = result.find(prefix) {
            let start = pos;
            let after = &result[pos + prefix.len()..];
            let end_offset = after
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',' || c == ';')
                .unwrap_or(after.len());

            if end_offset >= 8 {
                let key_end = pos + prefix.len() + end_offset;
                result = format!("{}[API_KEY]{}", &result[..start], &result[key_end..]);
            } else {
                break;
            }
        }
    }

    // Mask bearer tokens: "Bearer <token>" (case-insensitive)
    result = mask_bearer_tokens(&result);

    // Mask PEM private key blocks: "-----BEGIN * PRIVATE KEY-----"
    result = mask_private_key_blocks(&result);

    result
}

fn mask_bearer_tokens(text: &str) -> String {
    // Rebuild `lower` from `result` on every replacement to keep byte offsets
    // in sync with `result`.  This avoids the stale-offset bug that would
    // occur when successive replacements change the string length.
    let mut result = text.to_string();
    let needle = "bearer ";
    let mut search_from = 0usize;

    loop {
        // Always derive `lower` from the current `result` so offsets are correct.
        let lower = result.to_lowercase();
        if search_from >= lower.len() {
            break;
        }
        let Some(rel_pos) = lower[search_from..].find(needle) else {
            break;
        };
        let pos = search_from + rel_pos;
        let token_start = pos + needle.len();
        let after = &result[token_start..];
        let end_offset = after
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',' || c == ';')
            .unwrap_or(after.len());

        if end_offset >= 8 {
            let tail = result[token_start + end_offset..].to_string();
            result = format!("{}Bearer [API_KEY]{}", &result[..pos], tail);
            // Advance past the replacement so we do not re-examine it.
            search_from = pos + "bearer [api_key]".len();
        } else {
            search_from = token_start + end_offset.max(1);
        }
    }

    result
}

fn mask_private_key_blocks(text: &str) -> String {
    // Mask PEM-style private key headers: -----BEGIN * PRIVATE KEY-----
    if !text.contains("-----BEGIN ") || !text.contains("PRIVATE KEY-----") {
        return text.to_string();
    }

    let mut result = text.to_string();
    let mut search_from = 0;
    loop {
        let Some(rel_begin) = result[search_from..].find("-----BEGIN ") else {
            break;
        };
        let begin_pos = search_from + rel_begin;
        let header_after = &result[begin_pos + 11..];
        // Find the closing dashes of the header line
        let Some(header_end_rel) = header_after.find("-----") else {
            break;
        };
        let label = &header_after[..header_end_rel];
        if !label.contains("PRIVATE KEY") {
            // Not a private key block — skip past this marker and keep searching
            search_from = begin_pos + 11;
            continue;
        }
        let label = label.to_string();
        // Find the matching END marker
        let end_marker = format!("-----END {}-----", label);
        let block_end = result[begin_pos..].find(&end_marker);
        let replace_end = if let Some(rel) = block_end {
            begin_pos + rel + end_marker.len()
        } else {
            // No closing marker found — mask to end of string
            result.len()
        };
        let tail = result[replace_end..].to_string();
        result = format!("{}[PRIVATE_KEY]{}", &result[..begin_pos], tail);
        // After replacement, search_from stays at begin_pos (now points at [PRIVATE_KEY])
        search_from = begin_pos + "[PRIVATE_KEY]".len();
    }

    result
}

fn mask_ip_addresses(text: &str) -> String {
    let mut result = text.to_string();
    let chars: Vec<char> = result.chars().collect();
    let len = chars.len();
    let mut masked = String::new();
    let mut i = 0;

    while i < len {
        if chars[i].is_ascii_digit() {
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

    if i < len && chars[i].is_ascii_digit() {
        return None;
    }

    Some((i, octet_count == 4))
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn perf_gates_enabled() -> bool {
        std::env::var("ONESHIM_ENABLE_PERF_GATES")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    }

    fn perf_budget_ms(env_key: &str, default_ms: u128) -> u128 {
        std::env::var(env_key)
            .ok()
            .and_then(|value| value.parse::<u128>().ok())
            .unwrap_or(default_ms)
    }

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
        assert!(should_exclude(
            "Slack",
            "General",
            &["Slack".to_string()],
            &[],
            &[],
            false,
        ));
        assert!(should_exclude(
            "Chrome",
            "My Password Manager",
            &[],
            &[],
            &["*password*".to_string()],
            false,
        ));
    }

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

    #[test]
    fn mask_ipv4_basic() {
        let result = mask_ip_addresses("connecting to 10.0.0.1 on port 80");
        assert!(result.contains("[IP]"));
        assert!(!result.contains("10.0.0.1"));
    }

    #[test]
    fn mask_ipv4_preserves_non_ip() {
        let result = mask_ip_addresses("version 1.2.3");
        assert!(!result.contains("[IP]"));
    }

    #[test]
    fn mask_linux_home_path() {
        let result = mask_user_paths("/home/devuser/projects/secret.txt");
        assert!(result.contains("[USER]"));
        assert!(!result.contains("devuser"));
    }

    #[test]
    fn detect_pii_markers_in_email() {
        let markers =
            detect_pii_markers_with_level("contact: user@example.com", PiiFilterLevel::Standard);
        assert!(markers.contains(&PiiMarker::Email));
        assert!(is_sensitive_segment_with_level(
            "contact: user@example.com",
            PiiFilterLevel::Standard
        ));
    }

    #[test]
    fn no_markers_when_filter_off() {
        let markers =
            detect_pii_markers_with_level("contact: user@example.com", PiiFilterLevel::Off);
        assert!(markers.is_empty());
        assert!(!is_sensitive_segment_with_level(
            "contact: user@example.com",
            PiiFilterLevel::Off
        ));
    }

    #[test]
    fn perf_budget_privacy_marker_scan() {
        if !perf_gates_enabled() {
            return;
        }

        let samples = [
            "contact user@example.com for access",
            "call +82-10-1234-5678 for approval",
            "server 10.0.0.24 has alert",
            "token sk-abc123def456ghi789jkl0 detected",
            "normal productivity dashboard event",
            "file path /Users/alice/Documents/client-plan.md",
        ];

        let iterations = 3_000usize;
        let budget_ms = perf_budget_ms("ONESHIM_PERF_BUDGET_PRIVACY_SCAN_MS", 1_200);
        let started = Instant::now();
        let mut hits = 0usize;
        for _ in 0..iterations {
            for sample in samples {
                if is_sensitive_segment_with_level(sample, PiiFilterLevel::Strict) {
                    hits += 1;
                }
            }
        }
        let elapsed_ms = started.elapsed().as_millis();

        assert!(
            hits >= iterations * 5,
            "unexpected marker scan hit ratio: hits={} iterations={}",
            hits,
            iterations
        );
        assert!(
            elapsed_ms <= budget_ms,
            "privacy marker scan perf budget exceeded: elapsed={}ms budget={}ms",
            elapsed_ms,
            budget_ms
        );
    }

    #[test]
    fn mask_bearer_single_token() {
        let input = "Authorization: Bearer eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9";
        let result = sanitize_title_with_level(input, PiiFilterLevel::Strict);
        assert!(result.contains("Bearer [API_KEY]"), "single bearer not masked: {result}");
        assert!(!result.contains("eyJhbGci"), "raw token still present: {result}");
    }

    #[test]
    fn mask_bearer_multiple_tokens() {
        // Verifies that all bearer tokens in a string are masked, not just the first.
        let input =
            "first: Bearer eyJhbGciOiJSUzI1NiJ9 second: bearer zyxwvutsrqponmlkjih end";
        let result = sanitize_title_with_level(input, PiiFilterLevel::Strict);
        let count = result.matches("[API_KEY]").count();
        assert_eq!(count, 2, "expected 2 masked tokens, got {count}: {result}");
        assert!(!result.contains("eyJhbGci"), "first raw token still present: {result}");
        assert!(!result.contains("zyxwvuts"), "second raw token still present: {result}");
    }

    #[test]
    fn mask_bearer_case_insensitive() {
        let variations = [
            "BEARER ABCDEFGHIJKLMNOPQRST",
            "Bearer ABCDEFGHIJKLMNOPQRST",
            "bearer ABCDEFGHIJKLMNOPQRST",
            "BeArEr ABCDEFGHIJKLMNOPQRST",
        ];
        for input in &variations {
            let result = sanitize_title_with_level(input, PiiFilterLevel::Strict);
            assert!(
                result.contains("[API_KEY]"),
                "case variant not masked: {input} => {result}"
            );
        }
    }

    #[test]
    fn mask_bearer_short_token_not_masked() {
        // Tokens shorter than 8 chars must not be replaced.
        let input = "Authorization: Bearer short";
        let result = sanitize_title_with_level(input, PiiFilterLevel::Strict);
        assert!(!result.contains("[API_KEY]"), "short bearer should not be masked: {result}");
    }

    #[test]
    fn mask_private_key_block_single() {
        let input =
            "key: -----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA\n-----END RSA PRIVATE KEY-----";
        let result = sanitize_title_with_level(input, PiiFilterLevel::Strict);
        assert!(result.contains("[PRIVATE_KEY]"), "PEM block not masked: {result}");
        assert!(!result.contains("MIIEowIBAAKCAQEA"), "raw key material still present: {result}");
    }

    #[test]
    fn mask_private_key_block_non_private_unchanged() {
        // Public key blocks must not be masked.
        let input = "-----BEGIN PUBLIC KEY-----\nMIIBIjANBgkq\n-----END PUBLIC KEY-----";
        let result = sanitize_title_with_level(input, PiiFilterLevel::Strict);
        assert!(!result.contains("[PRIVATE_KEY]"), "public key should not be masked: {result}");
    }

    #[test]
    fn mask_ghs_token() {
        // ghs_ GitHub Actions token prefix added in this PR.
        let result = sanitize_title_with_level(
            "token: ghs_16C7e42F292c6912E7710c838347Ae178B4a",
            PiiFilterLevel::Strict,
        );
        assert!(result.contains("[API_KEY]"), "ghs_ token not masked: {result}");
        assert!(!result.contains("ghs_"), "raw ghs_ token still present: {result}");
    }
}
