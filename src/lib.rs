//! Office Open XML (Word / Excel) szövegjavítás: ismétlődő mondatok igazítása és szótár-alapú javaslatok.

pub mod dedup;
pub mod diff_view;
pub mod docx;
pub mod learn;
pub mod run;
pub mod spell;
pub mod xlsx;
pub(crate) mod zip_safe;

pub use diff_view::unified_git_style;
pub use learn::{default_learn_path, LearnedHabits};
pub use run::{default_output_for_input, run_fix, FixResult, RunParams};

use anyhow::{Context, Result};
use std::cell::RefCell;
use std::path::Path;

/// Kinyert szöveg diffhez (soronként / cellánként).
pub fn extract_plain_for_diff(data: &[u8], path: &Path) -> Result<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "docx" => docx::extract_plain_text(data).context("docx szöveg kinyerés"),
        "xlsx" => xlsx::extract_plain_text(data).context("xlsx szöveg kinyerés"),
        other => anyhow::bail!("diff: nem támogatott kiterjesztés: .{other}"),
    }
}

/// Futtatási beállítások.
#[derive(Debug, Clone)]
pub struct FixConfig {
    pub align_duplicates: bool,
    pub spell: bool,
    pub max_edit_distance: i64,
    /// Ha igaz, minden Word-bekezdés egyetlen `w:t`-be olvad (régi viselkedés).
    pub docx_merge_all_runs: bool,
}

impl Default for FixConfig {
    fn default() -> Self {
        Self {
            align_duplicates: true,
            spell: true,
            max_edit_distance: 2,
            docx_merge_all_runs: false,
        }
    }
}

/// Bemeneti fájl típusa kiterjesztés alapján.
pub fn fix_bytes(
    data: &[u8],
    path_hint: &Path,
    cfg: &FixConfig,
    engine: &spell::SpellEngine,
    habits: Option<&RefCell<learn::LearnedHabits>>,
) -> Result<Vec<u8>> {
    let ext = path_hint
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "docx" => docx::process_docx(data, cfg, engine, habits).context("docx feldolgozás"),
        "xlsx" => xlsx::process_xlsx(data, cfg, engine, habits).context("xlsx feldolgozás"),
        other => anyhow::bail!(
            "nem támogatott kiterjesztés: .{} (csak .docx és .xlsx)",
            other
        ),
    }
}

/// Tesztekhez / egyszerű szöveges bemenethez.
pub fn fix_plain_text(
    text: &str,
    cfg: &FixConfig,
    engine: &spell::SpellEngine,
    habits: Option<&RefCell<learn::LearnedHabits>>,
) -> String {
    let mut t = text.to_string();
    if cfg.align_duplicates {
        let map = dedup::build_canonical_map(&t);
        t = dedup::apply_alignment(&t, &map);
    }
    if cfg.spell {
        t = learn::fix_text_maybe_habits(&t, engine, habits, cfg.max_edit_distance);
    }
    t
}
