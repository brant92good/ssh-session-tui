//! Frozen Unicode case folding preserves legacy catalog identities and matching.
//!
//! Python 3.12/3.13 use Unicode 15.0/15.1. Their case-fold mappings are identical.
//! Lowercasing is insufficient (sharp S, final sigma, ligatures and Cherokee).
//! Do not switch this to a newer Unicode table without migrating state filenames.
mod table {
    include!("casefold_table.rs");
}

pub fn casefold(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for character in value.chars() {
        match table::FOLDS.binary_search_by_key(&(character as u32), |(key, _)| *key) {
            Ok(index) => result.push_str(table::FOLDS[index].1),
            Err(_) => result.push(character),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::casefold;

    #[test]
    fn full_default_folds_match_legacy_identity() {
        assert_eq!(
            casefold("Stra\u{df}e/\u{130}/\u{3c2}/\u{fb03}/\u{ab70}"),
            "strasse/i\u{307}/\u{3c3}/ffi/\u{13a0}"
        );
        assert_eq!(
            casefold("SERVER/\u{958b}\u{767c}"),
            "server/\u{958b}\u{767c}"
        );
    }
}
