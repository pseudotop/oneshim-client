use serde::{Deserialize, Serialize};

/// Bug report identifier for support correlation.
/// Format: `BUG-{12_hex_chars}` (16 chars total).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BugId(String);

impl BugId {
    pub fn new(id: String) -> Result<Self, &'static str> {
        if id.starts_with("BUG-")
            && id.len() == 16
            && id[4..].chars().all(|c| c.is_ascii_hexdigit())
        {
            Ok(Self(id))
        } else {
            Err("Bug ID must match format BUG-{12_hex_chars}")
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for BugId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_bug_id() {
        let id = BugId::new("BUG-a1b2c3d4e5f6".to_string());
        assert!(id.is_ok());
        assert_eq!(id.unwrap().as_str(), "BUG-a1b2c3d4e5f6");
    }

    #[test]
    fn rejects_short_id() {
        assert!(BugId::new("BUG-abc".to_string()).is_err());
    }

    #[test]
    fn rejects_wrong_prefix() {
        assert!(BugId::new("ERR-a1b2c3d4e5f6".to_string()).is_err());
    }

    #[test]
    fn rejects_non_hex_chars() {
        assert!(BugId::new("BUG-ghijklmnopqr".to_string()).is_err());
        assert!(BugId::new("BUG-<script>aaaa".to_string()).is_err());
    }

    #[test]
    fn rejects_too_long() {
        assert!(BugId::new("BUG-a1b2c3d4e5f6aa".to_string()).is_err());
    }

    #[test]
    fn serializes_as_string() {
        let id = BugId::new("BUG-a1b2c3d4e5f6".to_string()).unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"BUG-a1b2c3d4e5f6\"");
    }

    #[test]
    fn deserializes_from_string() {
        let id: BugId = serde_json::from_str("\"BUG-a1b2c3d4e5f6\"").unwrap();
        assert_eq!(id.as_str(), "BUG-a1b2c3d4e5f6");
    }

    #[test]
    fn display_impl() {
        let id = BugId::new("BUG-a1b2c3d4e5f6".to_string()).unwrap();
        assert_eq!(format!("{id}"), "BUG-a1b2c3d4e5f6");
    }
}
