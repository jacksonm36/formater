use anyhow::Result;
use clap::{Parser, ValueHint};
use formater::{run_fix, RunParams};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "formater", version, about = "Word (.docx) és Excel (.xlsx) szöveg: ismétlődő mondatok + szótár (EN/HU .dic)")]
struct Cli {
    #[arg(short, long, value_hint = ValueHint::FilePath)]
    input: PathBuf,

    #[arg(short, long, value_hint = ValueHint::FilePath)]
    output: Option<PathBuf>,

    #[arg(short = 'D', long, default_value = "dicts", value_hint = ValueHint::DirPath)]
    dict_dir: PathBuf,

    #[arg(long, value_hint = ValueHint::FilePath)]
    dic_hu: Option<PathBuf>,

    #[arg(long, value_hint = ValueHint::FilePath)]
    dic_en: Option<PathBuf>,

    #[arg(long)]
    no_align: bool,

    #[arg(long)]
    no_spell: bool,

    #[arg(long, default_value_t = 2)]
    max_edit_distance: i64,

    #[arg(long)]
    dry_run: bool,

    #[arg(long)]
    docx_merge_runs: bool,

    /// Kinyert szövegre git-stílusú összehasonlítás kiírása (stdout).
    #[arg(long)]
    git_diff: bool,

    /// Ne tanuljon a szótár által javasolt javításokból (nincs learned_habits.json frissítés).
    #[arg(long)]
    no_learn: bool,

    /// Tanulási JSON útvonal (alapértelmezés: %APPDATA%\\formater\\learned_habits.json).
    #[arg(long, value_hint = ValueHint::FilePath)]
    learn_file: Option<PathBuf>,

    #[cfg(feature = "hunspell")]
    #[arg(long)]
    native_hunspell: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let params = RunParams {
        input: cli.input.clone(),
        output: cli.output.clone(),
        dict_dir: cli.dict_dir.clone(),
        dic_hu: cli.dic_hu.clone(),
        dic_en: cli.dic_en.clone(),
        enable_align: !cli.no_align,
        enable_spell: !cli.no_spell,
        max_edit_distance: cli.max_edit_distance,
        docx_merge_all_runs: cli.docx_merge_runs,
        git_diff: cli.git_diff,
        learn_habits: !cli.no_learn,
        learn_path: cli.learn_file.clone(),
        #[cfg(feature = "hunspell")]
        native_hunspell: cli.native_hunspell,
    };

    let res = run_fix(&params, !cli.dry_run)?;
    if cli.dry_run {
        println!(
            "dry-run: {} → {} ({} bájt → {} bájt)",
            params.input.display(),
            res.output_path.display(),
            res.input_bytes,
            res.output_bytes
        );
        return Ok(());
    }
    println!("Kész: {}", res.output_path.display());
    if let Some(ref d) = res.unified_diff {
        println!("{d}");
    }
    Ok(())
}
