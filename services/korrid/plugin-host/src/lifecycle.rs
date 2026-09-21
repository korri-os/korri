use crate::declaration::validate_id;
use std::collections::{BTreeMap, BTreeSet};

/// Requiredness belongs to the release operation that knows the agreed device
/// behavior. It is supplied when the host is constructed. It is not stored in
/// plugin receipts or inferred from bundle membership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequiredPlugin {
    id: String,
    behavior: String,
}

impl RequiredPlugin {
    pub fn new(id: &str, behavior: &str) -> Result<Self, String> {
        validate_id(id)?;
        let behavior = behavior.trim();
        if behavior.is_empty() || behavior.len() > 256 || behavior.chars().any(char::is_control) {
            return Err("required behavior must be a short single-line description".into());
        }
        Ok(Self {
            id: id.into(),
            behavior: behavior.into(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LifecyclePolicy {
    Authoritative { required: BTreeMap<String, String> },
    RemovalUnavailable,
}

impl LifecyclePolicy {
    /// Construct the authoritative required-plugin view for one product or
    /// release operation. An empty list is permitted only when the caller has
    /// authoritatively determined that no behavior requires a plugin.
    pub fn new(required: Vec<RequiredPlugin>) -> Result<Self, String> {
        let mut by_id = BTreeMap::new();
        for plugin in required {
            if by_id.insert(plugin.id.clone(), plugin.behavior).is_some() {
                return Err(format!("required plugin {} is repeated", plugin.id));
            }
        }
        Ok(Self::Authoritative { required: by_id })
    }

    /// Fail closed for a caller that has no authoritative requiredness view.
    pub fn removal_unavailable() -> Self {
        Self::RemovalUnavailable
    }

    pub fn check_removal(&self, id: &str) -> Result<(), String> {
        validate_id(id)?;
        let required = match self {
            Self::Authoritative { required } => required,
            Self::RemovalUnavailable => {
                return Err(format!(
                    "cannot remove {id}: this caller has no authoritative lifecycle policy"
                ))
            }
        };
        if let Some(behavior) = required.get(id) {
            return Err(format!(
                "cannot remove {id}: required behavior {behavior:?} depends on this plugin"
            ));
        }
        Ok(())
    }
}

pub struct ReleaseUpdateReview {
    required: Vec<crate::package::Report>,
    optional_ids: Vec<String>,
}

impl ReleaseUpdateReview {
    pub fn required_reports(&self) -> &[crate::package::Report] {
        &self.required
    }

    pub fn optional_ids(&self) -> &[String] {
        &self.optional_ids
    }

    pub(crate) fn into_parts(self) -> (Vec<crate::package::Report>, Vec<String>) {
        (self.required, self.optional_ids)
    }
}

pub(crate) fn review_release_update(
    required: Vec<crate::package::Report>,
    optional: Vec<crate::package::Report>,
) -> Result<ReleaseUpdateReview, String> {
    let mut ids = BTreeSet::new();
    for report in &required {
        validate_id(&report.id)?;
        if !ids.insert(report.id.clone()) {
            return Err(format!("required plugin {} is repeated", report.id));
        }
    }
    let mut optional_ids = Vec::new();
    for report in optional {
        validate_id(&report.id)?;
        if ids.contains(&report.id) {
            return Err(format!(
                "plugin {} cannot be both required and optional",
                report.id
            ));
        }
        if !optional_ids.contains(&report.id) {
            optional_ids.push(report.id);
        }
    }
    Ok(ReleaseUpdateReview {
        required,
        optional_ids,
    })
}
