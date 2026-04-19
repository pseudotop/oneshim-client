//! IntegrityCode — Integrity 카테고리 에러 코드. `integrity.*` 접두사.

define_code_enum! {
    /// Integrity 카테고리 에러 코드.
    pub enum IntegrityCode {
        /// 바이너리 해시 불일치.
        HashMismatch => "integrity.hash_mismatch",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn as_str_round_trip_unique() {
        let codes: Vec<&str> = IntegrityCode::all().iter().map(|c| c.as_str()).collect();
        let unique: HashSet<_> = codes.iter().collect();
        assert_eq!(codes.len(), unique.len());
    }

    #[test]
    fn naming_convention() {
        for c in IntegrityCode::all() {
            let s = c.as_str();
            assert!(s
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch == '.' || ch == '_'));
            assert!(s.contains('.'));
        }
    }

    #[test]
    fn display_matches_as_str() {
        for c in IntegrityCode::all() {
            assert_eq!(format!("{c}"), c.as_str());
        }
    }
}
