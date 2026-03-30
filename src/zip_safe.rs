//! Zip slip és túl nagy OOXML bejegyzések ellenőrzése.

use anyhow::{bail, Result};

/// Egyetlen kibontott bejegyzés felső határa (zip bomb / memória védelem).
pub const MAX_OOXML_ENTRY_BYTES: usize = 80 * 1024 * 1024;

/// `..`, abszolút útvonal, túl hosszú név — elutasítva (CVE-szerű zip slip).
pub fn ooxml_zip_entry_name_ok(name: &str) -> bool {
    if name.is_empty() || name.len() > 512 {
        return false;
    }
    if name.starts_with('/') || name.starts_with('\\') {
        return false;
    }
    if name.contains(':') {
        return false;
    }
    let norm = name.replace('\\', "/");
    for seg in norm.split('/') {
        if seg == ".." {
            return false;
        }
    }
    true
}

pub fn ensure_ooxml_zip_entry(name: &str, uncompressed_len: usize) -> Result<()> {
    if !ooxml_zip_entry_name_ok(name) {
        bail!("biztonság: gyanús zip bejegyzésnév (zip slip?): {}", name);
    }
    if uncompressed_len > MAX_OOXML_ENTRY_BYTES {
        bail!(
            "biztonság: zip bejegyzés túl nagy ({} bájt): {}",
            uncompressed_len,
            name
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_parent_segments() {
        assert!(!ooxml_zip_entry_name_ok("../foo.xml"));
        assert!(!ooxml_zip_entry_name_ok("word/../../secret"));
        assert!(ooxml_zip_entry_name_ok("word/document.xml"));
    }
}
