//! Git-stílusú unified diff a kinyert szövegsorok között (szóköz- és sorváltozások látszanak).

use similar::TextDiff;

/// Soronkénti unified diff, mint a `git diff -u` (---/+++ fejléc, @@ kontextus).
///
/// A `-` sorok a **bemenet** (feldolgozás előtti kinyert szöveg), a `+` sorok a **kimenet**
/// (utána) — nem fordítva, ugyanúgy mint a `git diff`-ben.
pub fn unified_git_style(before: &str, after: &str) -> String {
    if before == after {
        return "(nincs eltérés a kinyert szövegsorok között)\n".to_string();
    }
    let diff = TextDiff::from_lines(before, after);
    let body = format!(
        "{}",
        diff.unified_diff()
            .context_radius(3)
            .header("bemenet (régi)", "kimenet (új)")
    );
    format!(
        "Diff jelölés: sor elején „-” = csak a bemeneten volt; „+” = csak a kimeneten (új érték).\n\
         A --- fejléc a régi, a +++ az új szöveget jelöli (mint git diff -u).\n\n\
         {body}"
    )
}
