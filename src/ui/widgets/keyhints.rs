//! keyhints widget (PLAN.md section 7).
//! KeyHints (PLAN.md 7.15).
pub fn hints(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, l)| format!("{k} {l}"))
        .collect::<Vec<_>>()
        .join("  ")
}
