//! thread_meters widget (PLAN.md section 7).
//! Per-thread meters (PLAN.md 7.8).
pub fn columns_for_width(inner_width: usize) -> usize {
    inner_width / 3
}
