//! CLI és GUI közös futtatási útvonal.

use crate::diff_view::unified_git_style;
use crate::extract_plain_for_diff;
use crate::fix_bytes;
use crate::learn::LearnedHabits;
use crate::spell::SpellEngine;
use crate::FixConfig;
use anyhow::{Context, Result};
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};

pub fn default_output_for_input(input: &Path) -> PathBuf {
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("out");
    let parent = input.parent().unwrap_or(Path::new("."));
    parent.join(format!("{}.fixed.{}", stem, ext))
}

pub fn collect_dict_paths(
    dict_dir: &Path,
    dic_hu: Option<PathBuf>,
    dic_en: Option<PathBuf>,
) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(p) = dic_hu {
        v.push(p);
    }
    if let Some(p) = dic_en {
        v.push(p);
    }
    if v.is_empty() {
        for name in ["hu_HU.dic", "en_US.dic", "en_GB.dic"] {
            let p = dict_dir.join(name);
            if p.exists() {
                v.push(p);
            }
        }
    }
    v
}

#[cfg(feature = "hunspell")]
pub fn aff_dic_pairs(dics: &[PathBuf]) -> Vec<(PathBuf, PathBuf)> {
    dics.iter()
        .filter_map(|dic| {
            let aff = dic.with_extension("aff");
            aff.exists().then_some((aff, dic.clone()))
        })
        .collect()
}

#[derive(Clone)]
pub struct RunParams {
    pub input: PathBuf,
    pub output: Option<PathBuf>,
    pub dict_dir: PathBuf,
    pub dic_hu: Option<PathBuf>,
    pub dic_en: Option<PathBuf>,
    pub enable_align: bool,
    pub enable_spell: bool,
    pub max_edit_distance: i64,
    pub docx_merge_all_runs: bool,
    /// Kinyert szövegre unified diff (git-stílus).
    pub git_diff: bool,
    /// Ugyanazt a szótár-javítást többször látva eltárolja és legközelebb előrébb veszi.
    pub learn_habits: bool,
    /// Tanulási JSON (üres = alapértelmezett hely: %APPDATA%\\formater\\learned_habits.json).
    pub learn_path: Option<PathBuf>,
    #[cfg(feature = "hunspell")]
    pub native_hunspell: bool,
}

pub struct FixResult {
    pub output_path: PathBuf,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub unified_diff: Option<String>,
}

/// Feldolgozás. `write_output`: ha hamis, nem ír fájlt (dry-run).
pub fn run_fix(params: &RunParams, write_output: bool) -> Result<FixResult> {
    let out_path =
        params
            .output
            .clone()
            .unwrap_or_else(|| default_output_for_input(&params.input));
    let dicts = collect_dict_paths(
        &params.dict_dir,
        params.dic_hu.clone(),
        params.dic_en.clone(),
    );

    #[cfg(feature = "hunspell")]
    let pairs = aff_dic_pairs(&dicts);

    let engine = if !params.enable_spell {
        SpellEngine::empty()
    } else {
        #[cfg(feature = "hunspell")]
        {
            if params.native_hunspell {
                if pairs.is_empty() {
                    anyhow::bail!(
                        "Natív Hunspell: legalább egy .dic kell, mellette ugyanolyan nevű .aff."
                    );
                }
                SpellEngine::from_hunspell_pairs(&pairs).context("hunspell betöltés")?
            } else {
                SpellEngine::from_paths_with_embedded(&dicts).context("szótár betöltés")?
            }
        }
        #[cfg(not(feature = "hunspell"))]
        {
            SpellEngine::from_paths_with_embedded(&dicts).context("szótár betöltés")?
        }
    };

    #[cfg(feature = "hunspell")]
    let spell_enabled = params.enable_spell
        && (!params.native_hunspell || !pairs.is_empty());
    #[cfg(not(feature = "hunspell"))]
    let spell_enabled = params.enable_spell;

    // Szótár távolság: túl nagy érték CPU/memória terhelés (CLI/GUI hibás input).
    let max_edit_distance = params.max_edit_distance.clamp(1, 5);

    let cfg = FixConfig {
        align_duplicates: params.enable_align,
        spell: spell_enabled,
        max_edit_distance,
        docx_merge_all_runs: params.docx_merge_all_runs,
    };

    let learn_file = if spell_enabled && params.learn_habits {
        Some(
            params
                .learn_path
                .clone()
                .unwrap_or_else(crate::learn::default_learn_path),
        )
    } else {
        None
    };

    let habits_cell: Option<RefCell<LearnedHabits>> = if let Some(ref p) = learn_file {
        Some(RefCell::new(
            LearnedHabits::load(p).with_context(|| format!("tanulási fájl: {}", p.display()))?,
        ))
    } else {
        None
    };

    let data =
        fs::read(&params.input).with_context(|| format!("olvasás: {}", params.input.display()))?;
    let input_bytes = data.len();
    let fixed = fix_bytes(
        &data,
        &params.input,
        &cfg,
        &engine,
        habits_cell.as_ref(),
    )?;

    if let (Some(cell), Some(p)) = (habits_cell.as_ref(), learn_file.as_ref()) {
        if cell.borrow().is_dirty() {
            cell.borrow()
                .save(p)
                .with_context(|| format!("tanulási fájl mentése: {}", p.display()))?;
        }
    }
    let output_bytes = fixed.len();

    let unified_diff = if params.git_diff {
        match (
            extract_plain_for_diff(&data, &params.input),
            extract_plain_for_diff(&fixed, &params.input),
        ) {
            (Ok(before), Ok(after)) => Some(unified_git_style(&before, &after)),
            _ => None,
        }
    } else {
        None
    };

    if write_output {
        if let Some(p) = out_path.parent() {
            fs::create_dir_all(p).ok();
        }
        fs::write(&out_path, &fixed).with_context(|| format!("írás: {}", out_path.display()))?;
    }
    Ok(FixResult {
        output_path: out_path,
        input_bytes,
        output_bytes,
        unified_diff,
    })
}
