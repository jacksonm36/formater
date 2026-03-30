use anyhow::{Context, Result};
use regex::Regex;
use std::borrow::Cow;
use std::path::Path;
use symspell::{SymSpell, UnicodeStringStrategy, Verbosity};

/// Beépített (exe-ben lévő) alap angol + teljes magyar Hunspell szólista (.dic).
const EMBEDDED_EN_DIC: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/embedded_en.dic"));
/// LibreOffice `hu_HU` szótár — lásd `assets/HU_DICTIONARY_SOURCE.txt`.
const EMBEDDED_HU_DIC: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/hu_HU_libreoffice.dic"));
/// Hunspell `.aff` nélkül hiányzó gyakori alakok (pl. *kicsit* a *kicsi* tőhöz képest).
const EMBEDDED_HU_SUPPLEMENT_DIC: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/embedded_hu_supplement.dic"));
/// Gyakori magyar szavak (OpenSubtitles alapú gyakorisági lista, MIT) — lásd `HU_DICTIONARY_SOURCE.txt`.
const EMBEDDED_HU_FREQUENCY_DIC: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/hu_frequency_hermitdave.dic"));
/// Magyar kettősbetűk (cs, dzs, …) önálló tokenként — `HUNGARIAN_ALPHABET.txt`.
const EMBEDDED_HU_ALPHABET_DIC: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/embedded_hu_alphabet.dic"));
/// Gyakori rövidítés- és mozaikszó-tokenek (AkH / MTA, Wikiforrás összefoglaló) — `HU_ORTHOGRAPHY_AKH.txt`.
const EMBEDDED_HU_ABBREV_DIC: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/embedded_hu_abbreviations.dic"));
/// IT / hálózat / biztonság: angol és magyar kiegészítő szótár (router, switch, tűzfal, VLAN, stb.).
const EMBEDDED_TECH_EN_DIC: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/embedded_tech_en.dic"));
const EMBEDDED_TECH_HU_DIC: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/embedded_tech_hu.dic"));

enum EngineInner {
    Symspell(SymSpell<UnicodeStringStrategy>),
    #[cfg(feature = "hunspell")]
    Hunspell(Vec<hunspell_rs::Hunspell>),
}

/// Szótár-alapú javítás: alapértelmezés SymSpell + `.dic`; opcionálisan natív Hunspell (`.aff`+`.dic`).
pub struct SpellEngine {
    inner: EngineInner,
}

impl SpellEngine {
    pub fn empty() -> Self {
        Self {
            inner: EngineInner::Symspell(SymSpell::default()),
        }
    }

    /// Csak külső `.dic` fájlok (régi viselkedés); a GUI/CLI alapból [`Self::from_paths_with_embedded`]-t használ.
    pub fn from_dic_paths(paths: &[std::path::PathBuf]) -> Result<Self> {
        let mut sym = SymSpell::default();
        for p in paths {
            if p.exists() {
                let words = parse_hunspell_dic(p).with_context(|| format!("dic: {}", p.display()))?;
                push_words_into_sym(&mut sym, &words);
            }
        }
        Ok(Self {
            inner: EngineInner::Symspell(sym),
        })
    }

    /// Beépített EN+HU szólista + opcionális további `.dic` útvonalak.
    pub fn from_paths_with_embedded(extra_dic_paths: &[std::path::PathBuf]) -> Result<Self> {
        let mut sym = SymSpell::default();
        push_words_into_sym(&mut sym, &parse_hunspell_dic_str(EMBEDDED_EN_DIC));
        push_words_into_sym(&mut sym, &parse_hunspell_dic_str(EMBEDDED_HU_DIC));
        push_words_into_sym(&mut sym, &parse_hunspell_dic_str(EMBEDDED_HU_SUPPLEMENT_DIC));
        push_words_into_sym(&mut sym, &parse_hunspell_dic_str(EMBEDDED_HU_FREQUENCY_DIC));
        push_words_into_sym(&mut sym, &parse_hunspell_dic_str(EMBEDDED_HU_ALPHABET_DIC));
        push_words_into_sym(&mut sym, &parse_hunspell_dic_str(EMBEDDED_HU_ABBREV_DIC));
        push_words_into_sym(&mut sym, &parse_hunspell_dic_str(EMBEDDED_TECH_EN_DIC));
        push_words_into_sym(&mut sym, &parse_hunspell_dic_str(EMBEDDED_TECH_HU_DIC));
        for p in extra_dic_paths {
            if p.exists() {
                let words = parse_hunspell_dic(p).with_context(|| format!("dic: {}", p.display()))?;
                push_words_into_sym(&mut sym, &words);
            }
        }
        Ok(Self {
            inner: EngineInner::Symspell(sym),
        })
    }

    /// Natív Hunspell: minden pár `.aff` + `.dic` (UTF-8 útvonal).
    #[cfg(feature = "hunspell")]
    pub fn from_hunspell_pairs(pairs: &[(std::path::PathBuf, std::path::PathBuf)]) -> Result<Self> {
        if pairs.is_empty() {
            anyhow::bail!("nincs egy érvényes .aff/.dic pár sem");
        }
        let mut v = Vec::with_capacity(pairs.len());
        for (aff, dic) in pairs {
            if !aff.exists() {
                anyhow::bail!(".aff nem található: {}", aff.display());
            }
            if !dic.exists() {
                anyhow::bail!(".dic nem található: {}", dic.display());
            }
            let aff_s = aff.to_str().context("aff útvonal nem UTF-8")?;
            let dic_s = dic.to_str().context("dic útvonal nem UTF-8")?;
            v.push(hunspell_rs::Hunspell::new(aff_s, dic_s));
        }
        Ok(Self {
            inner: EngineInner::Hunspell(v),
        })
    }

    pub fn correct_word(&self, word: &str, max_dist: i64) -> String {
        if word.chars().count() < 2 {
            return word.to_string();
        }
        match &self.inner {
            EngineInner::Symspell(sym) => correct_symspell(sym, word, max_dist),
            #[cfg(feature = "hunspell")]
            EngineInner::Hunspell(hs) => correct_hunspell(hs, word),
        }
    }
}

fn correct_symspell(
    sym: &SymSpell<UnicodeStringStrategy>,
    word: &str,
    max_dist: i64,
) -> String {
    let lower = word.to_lowercase();
    let sug = sym.lookup(&lower, Verbosity::Top, max_dist);
    if sug.is_empty() {
        return word.to_string();
    }
    let top = &sug[0];
    if top.distance == 0 {
        return word.to_string();
    }
    apply_case_pattern(word, top.term.as_str())
}

#[cfg(feature = "hunspell")]
fn correct_hunspell(instances: &[hunspell_rs::Hunspell], word: &str) -> String {
    use hunspell_rs::CheckResult;
    for h in instances {
        if h.check(word) == CheckResult::FoundInDictionary {
            return word.to_string();
        }
    }
    let lower = word.to_lowercase();
    for h in instances {
        if h.check(&lower) == CheckResult::FoundInDictionary {
            return apply_case_pattern(word, &lower);
        }
    }
    for h in instances {
        let sug = h.suggest(word);
        if let Some(best) = sug.first() {
            return apply_case_pattern(word, best.as_str());
        }
    }
    for h in instances {
        let sug = h.suggest(&lower);
        if let Some(best) = sug.first() {
            return apply_case_pattern(word, best.as_str());
        }
    }
    word.to_string()
}

fn push_words_into_sym(sym: &mut SymSpell<UnicodeStringStrategy>, words: &[String]) {
    for w in words {
        let line = format!("{} 1", w);
        sym.load_dictionary_line(&line, 0, 1, " ");
        let lower = w.to_lowercase();
        if lower != *w {
            sym.load_dictionary_line(&format!("{lower} 1"), 0, 1, " ");
        }
    }
}

pub fn parse_hunspell_dic_str(text: &str) -> Vec<String> {
    let mut lines = text.lines();
    let _first = lines.next();
    let mut words = Vec::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let base = line.split('/').next().unwrap_or(line);
        let w = base.split_whitespace().next().unwrap_or(base);
        if !w.is_empty() {
            words.push(w.to_string());
        }
    }
    words
}

pub fn parse_hunspell_dic(path: &Path) -> Result<Vec<String>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("olvasás: {}", path.display()))?;
    Ok(parse_hunspell_dic_str(&text))
}

pub fn apply_case_pattern(original: &str, replacement: &str) -> String {
    let has_letter = |s: &str| s.chars().any(|c| c.is_alphabetic());
    if !has_letter(original) || !has_letter(replacement) {
        return replacement.to_string();
    }
    let has_lower = original.chars().any(|c| c.is_lowercase());
    let has_upper = original.chars().any(|c| c.is_uppercase());
    if has_upper && !has_lower {
        return replacement.to_uppercase();
    }
    let mut rc = replacement.chars();
    let Some(f) = rc.next() else {
        return replacement.to_string();
    };
    let first_up = original
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false);
    let rest_lower = original
        .chars()
        .skip(1)
        .all(|c| !c.is_uppercase() || !c.is_alphabetic());
    if first_up && rest_lower {
        let mut s = f.to_uppercase().to_string();
        s.push_str(&rc.as_str().to_lowercase());
        return s;
    }
    replacement.to_string()
}

pub fn fix_text(text: &str, engine: &SpellEngine, max_dist: i64) -> String {
    let re = Regex::new(r"(?u)\p{L}[\p{L}']*").expect("regex");
    let mut out = String::new();
    let mut last = 0;
    for m in re.find_iter(text) {
        out.push_str(&text[last..m.start()]);
        out.push_str(&engine.correct_word(m.as_str(), max_dist));
        last = m.end();
    }
    out.push_str(&text[last..]);
    out
}

pub fn escape_xml_text(s: &str) -> String {
    quick_xml::escape::escape(Cow::Borrowed(s)).into_owned()
}

pub fn unescape_xml_text(s: &str) -> String {
    quick_xml::escape::unescape(s)
        .map(|c| c.into_owned())
        .unwrap_or_else(|_| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_paths_with_embedded_loads() {
        let engine = SpellEngine::from_paths_with_embedded(&[]).expect("embedded dic");
        let _ = engine;
    }

    #[test]
    fn correct_word_leaves_dictionary_tokens_unchanged() {
        let engine = SpellEngine::from_paths_with_embedded(&[]).expect("embedded dic");
        let max = 2_i64;
        for w in ["router", "firewall", "VLAN", "tűzfal", "switch"] {
            assert_eq!(
                engine.correct_word(w, max),
                w,
                "expected no correction for {w:?}"
            );
        }
    }

    #[test]
    fn hungarian_kicsit_szosz_not_stripped_to_shorter_words() {
        let engine = SpellEngine::from_paths_with_embedded(&[]).expect("embedded dic");
        let max = 2_i64;
        assert_eq!(engine.correct_word("kicsit", max), "kicsit");
        assert_eq!(engine.correct_word("szósz", max), "szósz");
    }

    #[test]
    fn hungarian_digraphs_recognized() {
        let engine = SpellEngine::from_paths_with_embedded(&[]).expect("embedded dic");
        let max = 2_i64;
        for w in ["cs", "dz", "dzs", "gy", "ly", "ny", "sz", "ty", "zs"] {
            assert_eq!(engine.correct_word(w, max), w, "{w}");
        }
    }

    #[test]
    fn hungarian_abbreviation_tokens_unchanged() {
        let engine = SpellEngine::from_paths_with_embedded(&[]).expect("embedded dic");
        let max = 2_i64;
        for w in ["stb", "kb", "szerk", "nato", "Bp", "OTP"] {
            assert_eq!(engine.correct_word(w, max), w, "{w}");
        }
    }

    #[test]
    fn parse_hunspell_dic_str_skips_header_comments_empty() {
        let text = "99\n# ignored\nalpha\n\nbeta/x SFX\n gamma tail\n";
        let words = parse_hunspell_dic_str(text);
        assert_eq!(words, vec!["alpha", "beta", "gamma"]);
    }
}
