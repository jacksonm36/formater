use crate::dedup;
use crate::learn::{self, LearnedHabits};
use crate::zip_safe;
use crate::spell::{escape_xml_text, unescape_xml_text, SpellEngine};
use crate::FixConfig;
use anyhow::{Context, Result};
use regex::{Captures, Regex};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::sync::OnceLock;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

fn re_si_block() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"(?s)<si\b[^>]*>.*?</si>"#).unwrap())
}

fn re_t() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"(?s)(<t\b[^>]*>)([^<]*)(</t>)"#).unwrap())
}

fn re_is_block() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"(?s)<is\b[^>]*>.*?</is>"#).unwrap())
}

/// Sorrend: sharedStrings összes `<si>`, majd `sheet1`, `sheet2`, … szerint minden inline `<is>`.
fn collect_workbook_plain_strings_for_map(
    shared_xml: Option<&str>,
    sheet_xmls: &[(&str, &str)],
) -> Vec<String> {
    let mut v = Vec::new();
    if let Some(xml) = shared_xml {
        v.extend(si_plains_in_order(xml));
    }
    for (_, xml) in sheet_xmls {
        v.extend(is_plains_in_order(xml));
    }
    v
}

fn si_plains_in_order(xml: &str) -> Vec<String> {
    re_si_block()
        .find_iter(xml)
        .map(|m| extract_si_plain(m.as_str()))
        .collect()
}

fn is_plains_in_order(xml: &str) -> Vec<String> {
    re_is_block()
        .find_iter(xml)
        .map(|m| extract_si_plain(m.as_str()))
        .collect()
}

fn worksheet_sort_key(path: &str) -> (u32, String) {
    let rest = path
        .strip_prefix("xl/worksheets/sheet")
        .unwrap_or(path);
    let num_str = rest.strip_suffix(".xml").unwrap_or(rest);
    let n = num_str.parse::<u32>().unwrap_or(u32::MAX);
    (n, path.to_string())
}

fn extract_si_plain(si: &str) -> String {
    let mut s = String::new();
    for c in re_t().captures_iter(si) {
        let inner = c.get(2).map(|m| m.as_str()).unwrap_or("");
        s.push_str(&unescape_xml_text(inner));
    }
    s
}

/// Egy Excel-cellán belül minden whitespace-sorozat egy szóközzé; sortörés nem maradhat
/// (különben a rácsban „üres soroknak” tűnik a többsoros cella).
fn normalize_xlsx_cell_text(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
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

fn transform_si_block(
    block: &str,
    plain: &str,
    cfg: &FixConfig,
    engine: &SpellEngine,
    map: Option<&HashMap<String, String>>,
    habits: Option<&RefCell<LearnedHabits>>,
) -> String {
    let mut new_plain = normalize_xlsx_cell_text(plain);
    if let Some(m) = map {
        new_plain = dedup::apply_alignment(&new_plain, m);
    }
    if cfg.spell {
        new_plain = spell_fix_plain(&new_plain, cfg, engine, habits);
    }
    new_plain = normalize_xlsx_cell_text(&new_plain);
    if block.contains("<r>") || block.contains("<r ") {
        return replace_first_t_only(block, &new_plain);
    }
    let re = re_t();
    let n = RefCell::new(0u32);
    re.replace_all(block, |caps: &Captures| {
        let mut c = n.borrow_mut();
        *c += 1;
        let body = if *c == 1 {
            escape_xml_text(&new_plain)
        } else {
            String::new()
        };
        format!(
            "{}{}{}",
            caps.get(1).unwrap().as_str(),
            body,
            caps.get(3).unwrap().as_str()
        )
    })
    .into_owned()
}

fn replace_first_t_only(block: &str, new_plain: &str) -> String {
    let re = re_t();
    let n = RefCell::new(0u32);
    re.replace_all(block, |caps: &Captures| {
        let mut c = n.borrow_mut();
        *c += 1;
        let body = if *c == 1 {
            escape_xml_text(new_plain)
        } else {
            caps.get(2).map(|m| m.as_str()).unwrap_or("").to_string()
        };
        format!(
            "{}{}{}",
            caps.get(1).unwrap().as_str(),
            body,
            caps.get(3).unwrap().as_str()
        )
    })
    .into_owned()
}

fn transform_shared_strings_xml(
    xml: &str,
    cfg: &FixConfig,
    engine: &SpellEngine,
    map: Option<&HashMap<String, String>>,
    habits: Option<&RefCell<LearnedHabits>>,
) -> String {
    let plains: Vec<String> = si_plains_in_order(xml);
    let idx = RefCell::new(0usize);
    re_si_block()
        .replace_all(xml, |caps: &Captures| {
            let block = caps.get(0).unwrap().as_str();
            let mut i = idx.borrow_mut();
            let plain = plains.get(*i).map(String::as_str).unwrap_or("");
            *i += 1;
            transform_si_block(block, plain, cfg, engine, map, habits)
        })
        .into_owned()
}

fn transform_inline_sheet_xml(
    xml: &str,
    cfg: &FixConfig,
    engine: &SpellEngine,
    map: Option<&HashMap<String, String>>,
    habits: Option<&RefCell<LearnedHabits>>,
) -> String {
    let plains: Vec<String> = is_plains_in_order(xml);
    let idx = RefCell::new(0usize);
    re_is_block()
        .replace_all(xml, |caps: &Captures| {
            let block = caps.get(0).unwrap().as_str();
            let mut i = idx.borrow_mut();
            let plain = plains.get(*i).map(String::as_str).unwrap_or("");
            *i += 1;
            transform_si_block(block, plain, cfg, engine, map, habits)
        })
        .into_owned()
}

pub fn process_xlsx(
    data: &[u8],
    cfg: &FixConfig,
    engine: &SpellEngine,
    habits: Option<&RefCell<LearnedHabits>>,
) -> Result<Vec<u8>> {
    let cursor = Cursor::new(data);
    let mut archive = ZipArchive::new(cursor)?;

    let mut files: Vec<(String, Vec<u8>, CompressionMethod, DateTime)> =
        Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name().to_owned();
        let compression = file.compression();
        let lmt = file
            .last_modified()
            .unwrap_or_else(DateTime::default_for_write);
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        zip_safe::ensure_ooxml_zip_entry(&name, buf.len())?;
        files.push((name, buf, compression, lmt));
    }

    let shared_xml = files
        .iter()
        .find(|(n, _, _, _)| n == "xl/sharedStrings.xml")
        .map(|(_, b, _, _)| String::from_utf8_lossy(b).into_owned());

    let mut sheet_entries: Vec<(String, String)> = files
        .iter()
        .filter(|(n, _, _, _)| n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml"))
        .map(|(n, b, _, _)| (n.clone(), String::from_utf8_lossy(b).into_owned()))
        .collect();
    sheet_entries.sort_by_key(|(n, _)| worksheet_sort_key(n));

    let sheet_refs: Vec<(&str, &str)> = sheet_entries
        .iter()
        .map(|(n, x)| (n.as_str(), x.as_str()))
        .collect();

    let global_map = if cfg.align_duplicates {
        let plains = collect_workbook_plain_strings_for_map(
            shared_xml.as_deref(),
            &sheet_refs,
        );
        if plains.is_empty() {
            None
        } else {
            Some(dedup::build_canonical_map_from_paragraphs(&plains))
        }
    } else {
        None
    };

    let mut out = Cursor::new(Vec::new());
    {
        let mut zip_out = ZipWriter::new(&mut out);
        for (name, buf, compression, lmt) in files {
            let out_data = if name == "xl/sharedStrings.xml" {
                let xml = String::from_utf8_lossy(&buf).into_owned();
                transform_shared_strings_xml(
                    &xml,
                    cfg,
                    engine,
                    global_map.as_ref(),
                    habits,
                )
                .into_bytes()
            } else if name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml") {
                let xml = String::from_utf8_lossy(&buf).into_owned();
                transform_inline_sheet_xml(
                    &xml,
                    cfg,
                    engine,
                    global_map.as_ref(),
                    habits,
                )
                .into_bytes()
            } else {
                buf
            };
            let opts = SimpleFileOptions::default()
                .compression_method(compression)
                .last_modified_time(lmt);
            zip_out.start_file(name, opts)?;
            zip_out.write_all(&out_data)?;
        }
        zip_out.finish()?;
    }
    Ok(out.into_inner())
}

/// Ugyanaz a szövegsorrend, mint a globális mondat-térképnél (sharedStrings + lapok).
pub fn extract_plain_text(data: &[u8]) -> Result<String> {
    let cursor = Cursor::new(data);
    let mut archive = ZipArchive::new(cursor).context("xlsx zip")?;
    let mut files: Vec<(String, Vec<u8>)> = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).context("xlsx zip bejegyzés")?;
        let name = file.name().to_owned();
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).context("xlsx olvasás")?;
        zip_safe::ensure_ooxml_zip_entry(&name, buf.len()).context("xlsx zip bejegyzés")?;
        files.push((name, buf));
    }
    let shared_xml = files
        .iter()
        .find(|(n, _)| n == "xl/sharedStrings.xml")
        .map(|(_, b)| String::from_utf8_lossy(b).into_owned());
    let mut sheet_entries: Vec<(String, String)> = files
        .iter()
        .filter(|(n, _)| n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml"))
        .map(|(n, b)| (n.clone(), String::from_utf8_lossy(b).into_owned()))
        .collect();
    sheet_entries.sort_by_key(|(n, _)| worksheet_sort_key(n));
    let sheet_refs: Vec<(&str, &str)> = sheet_entries
        .iter()
        .map(|(n, x)| (n.as_str(), x.as_str()))
        .collect();
    let plains = collect_workbook_plain_strings_for_map(shared_xml.as_deref(), &sheet_refs);
    Ok(plains.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::normalize_xlsx_cell_text;

    #[test]
    fn xlsx_cell_text_strips_newlines() {
        assert_eq!(
            normalize_xlsx_cell_text("nem nagy\n\n"),
            "nem nagy"
        );
        assert_eq!(
            normalize_xlsx_cell_text("a  \n  b"),
            "a b"
        );
    }
}
