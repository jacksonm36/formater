use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;

/// Mondat kulcs: kisbetű, egységes szóköz.
pub fn normalize_key(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Első előfordulás szövege marad az etalon minden azonos (normalizált) mondatra.
pub fn build_canonical_map(text: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for seg in text.split_sentence_bounds() {
        if !seg.chars().any(|c| c.is_alphanumeric()) {
            continue;
        }
        let k = normalize_key(seg);
        m.entry(k).or_insert_with(|| seg.to_string());
    }
    m
}

/// Több szövegből egy közös térkép (sorrend: ahogy a slice-ban vannak).
///
/// Nem fűzi össze őket `\n\n`-nel — az mesterséges sortöréseket vinne az etalon
/// szövegekbe (pl. Excel cellák), és a cellában „üres soroknak” látszana.
pub fn build_canonical_map_from_paragraphs(paragraphs: &[String]) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for p in paragraphs {
        for (k, v) in build_canonical_map(p) {
            m.entry(k).or_insert(v);
        }
    }
    m
}

/// A térkép alapján cseréli az ismétlődő mondatokat az etalonra.
pub fn apply_alignment(text: &str, map: &HashMap<String, String>) -> String {
    let mut out = String::new();
    for seg in text.split_sentence_bounds() {
        if seg.chars().any(|c| c.is_alphanumeric()) {
            let k = normalize_key(seg);
            if let Some(canon) = map.get(&k) {
                out.push_str(canon);
                continue;
            }
        }
        out.push_str(seg);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligns_duplicate_sentences() {
        let s = "Hello world. Hello  world. End.";
        let m = build_canonical_map(s);
        let out = apply_alignment(s, &m);
        assert!(out.contains("Hello world."));
        assert!(!out.contains("Hello  world"));
    }

    #[test]
    fn merged_paragraph_map_has_no_join_newlines() {
        let cells = vec![
            "nem nagy".to_string(),
            "nem nagy".to_string(),
            "x".to_string(),
        ];
        let m = build_canonical_map_from_paragraphs(&cells);
        let canon = m.get(&normalize_key("nem nagy")).unwrap();
        assert!(
            !canon.contains('\n'),
            "etalon nem tartalmazhat rejtett sortörést: {canon:?}"
        );
        let out = apply_alignment("nem nagy", &m);
        assert!(!out.contains('\n'));
    }
}
