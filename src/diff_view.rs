//! Git-stílusú unified diff a kinyert szövegsorok között (szóköz- és sorváltozások látszanak).

use similar::TextDiff;

/// Soronkénti unified diff, mint a `git diff -u` (---/+++ fejléc, @@ kontextus).
pub fn unified_git_style(before: &str, after: &str) -> String {
    if before == after {
        return "(nincs eltérés a kinyert szövegsorok között)\n".to_string();
    }
    let diff = TextDiff::from_lines(before, after);
    format!(
        "{}",
        diff.unified_diff()
            .context_radius(3)
            .header("bemenet", "kimenet")
    )
}
