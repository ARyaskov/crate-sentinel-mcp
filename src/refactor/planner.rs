use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub struct RefactorAction {
    pub action_type: String,
    pub old_path: String,
    pub new_path: String,
}

#[derive(Debug, Clone)]
pub struct RefactorPlan {
    pub breaking: bool,
    pub actions: Vec<RefactorAction>,
}

pub fn build_plan(details: &[(String, String)], breaking: bool) -> RefactorPlan {
    let mut unique = BTreeSet::<(String, String, String)>::new();
    for (kind, path) in details {
        let action = match kind.as_str() {
            "removed_item" => ("remove_usage", path.clone(), String::new()),
            "changed_signature" => ("update_signature", path.clone(), String::new()),
            "trait_bound_change" => ("update_trait_bounds", path.clone(), String::new()),
            "visibility_change" => ("adjust_visibility", path.clone(), String::new()),
            _ => ("inspect_change", path.clone(), String::new()),
        };
        unique.insert((action.0.to_string(), action.1, action.2));
    }

    let actions = unique
        .into_iter()
        .map(|(action_type, old_path, new_path)| RefactorAction {
            action_type,
            old_path,
            new_path,
        })
        .collect();
    RefactorPlan { breaking, actions }
}
