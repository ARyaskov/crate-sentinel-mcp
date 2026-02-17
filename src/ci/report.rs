#[derive(Debug, Clone)]
pub struct UpdateRow {
    pub name: String,
    pub allowed: bool,
    pub disallowed_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CiDisallowedItem {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct CiSummary {
    pub updates_total: usize,
    pub updates_allowed: usize,
    pub updates_disallowed: usize,
    pub disallowed: Vec<CiDisallowedItem>,
}

pub fn summarize(updates: &[UpdateRow]) -> CiSummary {
    let updates_total = updates.len();
    let updates_allowed = updates.iter().filter(|entry| entry.allowed).count();
    let updates_disallowed = updates_total.saturating_sub(updates_allowed);
    let mut disallowed = updates
        .iter()
        .filter(|entry| !entry.allowed)
        .map(|entry| CiDisallowedItem {
            name: entry.name.clone(),
            reason: entry
                .disallowed_reason
                .clone()
                .unwrap_or_else(|| "disallowed".to_string()),
        })
        .collect::<Vec<_>>();
    disallowed.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.reason.cmp(&right.reason))
    });

    CiSummary {
        updates_total,
        updates_allowed,
        updates_disallowed,
        disallowed,
    }
}
