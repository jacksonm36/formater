//! Ismétlődő javítási szokások: ugyanabból a típushibából többször ugyanazt a javítást rögzíti,
//! és a szótár előtt alkalmazza (személyre szabott korrekciók).

use crate::spell::{apply_case_pattern, SpellEngine};
use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const FILE_VERSION: u32 = 1;

/// Tanulási JSON max. fájlméret (DoS / memória ellen).
const MAX_LEARN_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// Max. betanult pár (corrections).
const MAX_CORRECTION_ENTRIES: usize = 50_000;

/// Max. pending kulcsok száma.
const MAX_PENDING_KEYS: usize = 20_000;

/// Max. karakter egy szópárban (kulcs vagy érték).
const MAX_LEARN_WORD_CHARS: usize = 256;

/// Hányszor kell ugyanazt az (eredeti → javított) párost látni, mielőtt véglegesen eltároljuk.
const DEFAULT_CONFIRM_THRESHOLD: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LearnedHabitsFile {
    #[serde(default = "file_version")]
    version: u32,
    #[serde(default = "default_threshold")]
    threshold: u32,
    #[serde(default)]
    corrections: HashMap<String, String>,
    /// Eredeti (kisbetű) → javított (kisbetű) → előfordulások száma (még nem végleges).
    #[serde(default)]
    pending: HashMap<String, HashMap<String, u32>>,
}

fn default_threshold() -> u32 {
    DEFAULT_CONFIRM_THRESHOLD
}

fn file_version() -> u32 {
    FILE_VERSION
}

fn validate_habits_payload(f: &LearnedHabitsFile) -> Result<()> {
    if f.corrections.len() > MAX_CORRECTION_ENTRIES {
        anyhow::bail!("túl sok betanult pár (max {MAX_CORRECTION_ENTRIES})");
    }
    if f.pending.len() > MAX_PENDING_KEYS {
        anyhow::bail!("túl sok függő kulcs (max {MAX_PENDING_KEYS})");
    }
    let mut inner_entries = 0usize;
    for (ok, inner) in &f.pending {
        inner_entries = inner_entries.saturating_add(inner.len());
        if inner_entries > MAX_CORRECTION_ENTRIES {
            anyhow::bail!("túl sok függő bejegyzés");
        }
        if ok.len() > MAX_LEARN_WORD_CHARS {
            anyhow::bail!("tanulási kulcs túl hosszú");
        }
        for (ik, iv) in inner {
            if ik.len() > MAX_LEARN_WORD_CHARS {
                anyhow::bail!("érvénytelen pending mező");
            }
            if *iv > 1_000_000 {
                anyhow::bail!("érvénytelen számláló");
            }
        }
    }
    for (k, v) in &f.corrections {
        if k.len() > MAX_LEARN_WORD_CHARS || v.len() > MAX_LEARN_WORD_CHARS {
            anyhow::bail!("tanulási szó túl hosszú (max {MAX_LEARN_WORD_CHARS} karakter)");
        }
    }
    Ok(())
}

/// Betanult szótár-javítások + megfigyelések.
#[derive(Debug, Clone)]
pub struct LearnedHabits {
    threshold: u32,
    corrections: HashMap<String, String>,
    pending: HashMap<String, HashMap<String, u32>>,
    dirty: bool,
}

impl LearnedHabits {
    pub fn empty() -> Self {
        Self {
            threshold: DEFAULT_CONFIRM_THRESHOLD,
            corrections: HashMap::new(),
            pending: HashMap::new(),
            dirty: false,
        }
    }

    /// Hányszor kell ismételni ugyanazt a javítást, mielőtt végleges szabály lesz (alapértelmezés: 2).
    pub fn with_confirm_threshold(threshold: u32) -> Self {
        Self {
            threshold: threshold.max(1),
            corrections: HashMap::new(),
            pending: HashMap::new(),
            dirty: false,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Azonnali szabály (nem kell kétszer megismételni): `typo` → `preferred` (kis-nagybetű a szövegben megmarad).
    pub fn remember_immediately(&mut self, typo: &str, preferred: &str) {
        let k = typo.to_lowercase();
        self.corrections
            .insert(k.clone(), preferred.to_lowercase());
        self.pending.remove(&k);
        self.dirty = true;
    }

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::empty());
        }
        let meta = fs::metadata(path).with_context(|| format!("tanulási fájl: {}", path.display()))?;
        if meta.len() > MAX_LEARN_FILE_BYTES {
            anyhow::bail!(
                "tanulási fájl túl nagy (max {} bájt): {}",
                MAX_LEARN_FILE_BYTES,
                path.display()
            );
        }
        let s = fs::read_to_string(path)
            .with_context(|| format!("tanulási fájl olvasása: {}", path.display()))?;
        let f: LearnedHabitsFile = serde_json::from_str(&s).with_context(|| {
            format!(
                "tanulási fájl JSON (várható séma v{}): {}",
                FILE_VERSION,
                path.display()
            )
        })?;
        validate_habits_payload(&f).with_context(|| format!("tanulási fájl: {}", path.display()))?;
        Ok(Self {
            threshold: f.threshold.max(1),
            corrections: f.corrections,
            pending: f.pending,
            dirty: false,
        })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).with_context(|| format!("mappa: {}", dir.display()))?;
        }
        let f = LearnedHabitsFile {
            version: FILE_VERSION,
            threshold: self.threshold,
            corrections: self.corrections.clone(),
            pending: self.pending.clone(),
        };
        let json = serde_json::to_string_pretty(&f).context("JSON sorosítás")?;
        fs::write(path, json).with_context(|| format!("tanulási fájl írása: {}", path.display()))?;
        Ok(())
    }

    fn preferred_lower_owned(&self, word: &str) -> Option<String> {
        let k = word.to_lowercase();
        self.corrections.get(&k).cloned()
    }

    /// Rögzíti, hogy a szótár `original`-t `corrected`-re változtatta.
    pub fn observe(&mut self, original: &str, corrected: &str) {
        if original == corrected {
            return;
        }
        let ol = original.to_lowercase();
        let cl = corrected.to_lowercase();
        if ol == cl {
            return;
        }
        if self.corrections.contains_key(&ol) {
            return;
        }
        if self.corrections.len() >= MAX_CORRECTION_ENTRIES {
            return;
        }
        if !self.pending.contains_key(&ol) && self.pending.len() >= MAX_PENDING_KEYS {
            return;
        }

        let inner = self.pending.entry(ol.clone()).or_default();
        *inner.entry(cl.clone()).or_insert(0) += 1;
        let count = *inner.get(&cl).unwrap_or(&0);
        if count >= self.threshold {
            self.pending.remove(&ol);
            self.corrections.insert(ol, cl);
        }
        self.dirty = true;
    }
}

/// Alapértelmezett tanulási JSON: Windows `%APPDATA%\formater\learned_habits.json`, egyébként XDG vagy `~/.config`.
pub fn default_learn_path() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata)
                .join("formater")
                .join("learned_habits.json");
        }
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg)
            .join("formater")
            .join("learned_habits.json");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("formater")
            .join("learned_habits.json");
    }
    PathBuf::from("learned_habits.json")
}

pub fn fix_spell_token(
    engine: &SpellEngine,
    habits: Option<&RefCell<LearnedHabits>>,
    word: &str,
    max_dist: i64,
) -> String {
    if word.chars().count() < 2 {
        return word.to_string();
    }
    if let Some(cell) = habits {
        if let Some(pref) = cell.borrow().preferred_lower_owned(word) {
            return apply_case_pattern(word, &pref);
        }
    }
    let corrected = engine.correct_word(word, max_dist);
    if let Some(cell) = habits {
        if corrected != word {
            cell.borrow_mut().observe(word, &corrected);
        }
    }
    corrected
}

/// Mint [`crate::spell::fix_text`], de betanult szabályokkal és megfigyeléssel.
pub fn fix_text_maybe_habits(
    text: &str,
    engine: &SpellEngine,
    habits: Option<&RefCell<LearnedHabits>>,
    max_dist: i64,
) -> String {
    let re = Regex::new(r"(?u)\p{L}[\p{L}']*").expect("regex");
    let mut out = String::new();
    let mut last = 0;
    for m in re.find_iter(text) {
        out.push_str(&text[last..m.start()]);
        out.push_str(&fix_spell_token(engine, habits, m.as_str(), max_dist));
        last = m.end();
    }
    out.push_str(&text[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spell::SpellEngine;

    #[test]
    fn observe_promotes_after_threshold() {
        let mut h = LearnedHabits::with_confirm_threshold(2);
        h.observe("roteur", "router");
        assert!(!h.corrections.contains_key("roteur"));
        h.observe("roteur", "router");
        assert_eq!(h.corrections.get("roteur").map(String::as_str), Some("router"));
        assert!(h.is_dirty());
    }

    #[test]
    fn learned_override_before_engine() {
        let mut h = LearnedHabits::empty();
        h.remember_immediately("roteur", "router");
        let engine = SpellEngine::empty();
        let cell = RefCell::new(h);
        let out = fix_spell_token(&engine, Some(&cell), "Roteur", 2);
        assert_eq!(out, "Router");
    }
}
