use crate::dedup;
use crate::learn::{self, LearnedHabits};
use crate::zip_safe;
use crate::spell::{escape_xml_text, unescape_xml_text, SpellEngine};
use crate::FixConfig;
use anyhow::{Context, Result};
use regex::{Captures, Regex};
use std::cell::RefCell;
use std::io::{Cursor, Read, Write};
use std::sync::OnceLock;
use zip::write::SimpleFileOptions;
use zip::{DateTime, ZipArchive, ZipWriter};

fn re_wp_inner() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"(?s)<w:p\b[^>]*>(.*?)</w:p>"#).unwrap())
}

fn re_wt() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"<w:t(\b[^>]*?)>([^<]*)</w:t>"#).unwrap())
}

fn re_wp_full() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"(?s)(<w:p\b[^>]*>)(.*?)(</w:p>)"#).unwrap())
}

fn decode_concat_wt(inner: &str) -> String {
    let mut s = String::new();
    for c in re_wt().captures_iter(inner) {
        let t = c.get(2).map(|m| m.as_str()).unwrap_or("");
        s.push_str(&unescape_xml_text(t));
    }
    s
}

fn count_wt(inner: &str) -> usize {
    re_wt().captures_iter(inner).count()
}

fn rewrite_w_t_runs(inner: &str, new_plain: &str) -> String {
    let re = re_wt();
    let n = RefCell::new(0u32);
    re.replace_all(inner, |caps: &Captures| {
        let mut count = n.borrow_mut();
        *count += 1;
        let attrs = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let body = if *count == 1 {
            escape_xml_text(new_plain)
        } else {
            String::new()
        };
        format!("<w:t{}>{}</w:t>", attrs, body)
    })
    .into_owned()
}

fn spell_fix_plain(
    s: &str,
    cfg: &FixConfig,
    engine: &SpellEngine,
    habits: Option<&RefCell<LearnedHabits>>,
) -> String {
    if !cfg.spell {
        return s.to_string();
    }
    learn::fix_text_maybe_habits(s, engine, habits, cfg.max_edit_distance)
}

/// Több `w:t` futás megőrzése: javítás futásonként (dedup nincs ilyen bekezdésre).
fn rewrite_each_w_t(
    inner: &str,
    cfg: &FixConfig,
    engine: &SpellEngine,
    habits: Option<&RefCell<LearnedHabits>>,
) -> String {
    let re = re_wt();
    re.replace_all(inner, |caps: &Captures| {
        let attrs = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let text = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        let dec = unescape_xml_text(text);
        let fixed = spell_fix_plain(&dec, cfg, engine, habits);
        format!("<w:t{}>{}</w:t>", attrs, escape_xml_text(&fixed))
    })
    .into_owned()
}

fn process_word_ml(
    xml: &str,
    cfg: &FixConfig,
    engine: &SpellEngine,
    habits: Option<&RefCell<LearnedHabits>>,
) -> String {
    let plains: Vec<String> = re_wp_inner()
        .captures_iter(xml)
        .map(|c| decode_concat_wt(c.get(1).unwrap().as_str()))
        .collect();
    let map = if cfg.align_duplicates && !plains.is_empty() {
        Some(dedup::build_canonical_map_from_paragraphs(&plains))
    } else {
        None
    };
    let idx = RefCell::new(0usize);
    re_wp_full()
        .replace_all(xml, |caps: &Captures| {
            let mut i = idx.borrow_mut();
            let inner = caps.get(2).unwrap().as_str();
            let plain = plains.get(*i).map(String::as_str).unwrap_or("");
            *i += 1;
            let merge = cfg.docx_merge_all_runs || count_wt(inner) <= 1;
            let new_inner = if merge {
                let mut new_plain = plain.to_string();
                if let Some(ref m) = map {
                    new_plain = dedup::apply_alignment(&new_plain, m);
                }
                if cfg.spell {
                    new_plain =
                        spell_fix_plain(&new_plain, cfg, engine, habits);
                }
                rewrite_w_t_runs(inner, &new_plain)
            } else {
                rewrite_each_w_t(inner, cfg, engine, habits)
            };
            format!(
                "{}{}{}",
                caps.get(1).unwrap().as_str(),
                new_inner,
                caps.get(3).unwrap().as_str()
            )
        })
        .into_owned()
}

fn should_patch_word_xml(name: &str) -> bool {
    if !name.starts_with("word/") || !name.ends_with(".xml") {
        return false;
    }
    let base = name.trim_start_matches("word/");
    base.starts_with("document")
        || base.starts_with("header")
        || base.starts_with("footer")
        || base.starts_with("footnotes")
        || base.starts_with("endnotes")
}

/// Kinyert látható szöveg (bekezdésenként egy sor) — diff előnézethez.
pub fn extract_plain_text(data: &[u8]) -> Result<String> {
    let cursor = Cursor::new(data);
    let mut archive = ZipArchive::new(cursor).context("docx zip")?;
    let mut lines_out: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).context("docx zip bejegyzés")?;
        let name = file.name().to_owned();
        if !should_patch_word_xml(&name) {
            continue;
        }
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).context("docx olvasás")?;
        zip_safe::ensure_ooxml_zip_entry(&name, buf.len()).context("docx zip bejegyzés")?;
        let xml = String::from_utf8_lossy(&buf);
        for cap in re_wp_inner().captures_iter(&xml) {
            let inner = cap.get(1).unwrap().as_str();
            lines_out.push(decode_concat_wt(inner));
        }
    }
    Ok(lines_out.join("\n"))
}

pub fn process_docx(
    data: &[u8],
    cfg: &FixConfig,
    engine: &SpellEngine,
    habits: Option<&RefCell<LearnedHabits>>,
) -> Result<Vec<u8>> {
    let cursor = Cursor::new(data);
    let mut archive = ZipArchive::new(cursor)?;
    let mut out = Cursor::new(Vec::new());
    {
        let mut zip_out = ZipWriter::new(&mut out);
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let name = file.name().to_owned();
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            zip_safe::ensure_ooxml_zip_entry(&name, buf.len())?;
            let out_data = if should_patch_word_xml(&name) {
                let xml = String::from_utf8_lossy(&buf).into_owned();
                process_word_ml(&xml, cfg, engine, habits).into_bytes()
            } else {
                buf
            };
            let opts = SimpleFileOptions::default()
                .compression_method(file.compression())
                .last_modified_time(
                    file
                        .last_modified()
                        .unwrap_or_else(DateTime::default_for_write),
                );
            zip_out.start_file(name, opts)?;
            zip_out.write_all(&out_data)?;
        }
        zip_out.finish()?;
    }
    Ok(out.into_inner())
}
