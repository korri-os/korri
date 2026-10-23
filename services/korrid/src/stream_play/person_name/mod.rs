//! The person-authored app-data record. The wire shape is contract O6.
pub(crate) mod device;
use crate::identity::DeviceIdentity;
use serde::{Deserialize, Serialize};

pub(crate) const SETTING_ID: &str = "person-name";
pub(crate) const KIND: u16 = 30_078;
const D_TAG: &str = "korri.person-name";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Content {
    name: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Template {
    pub kind: u16,
    pub created_at: u64,
    pub tags: Vec<Vec<String>>,
    pub content: String,
}

pub(crate) fn template(name: &str, created_at: u64) -> Result<String, String> {
    validate_name(name)?;
    serde_json::to_string(&Template {
        kind: KIND,
        created_at,
        tags: vec![vec!["d".into(), D_TAG.into()]],
        content: serde_json::to_string(&Content { name: name.into() })
            .map_err(|e| e.to_string())?,
    })
    .map_err(|e| e.to_string())
}

pub(crate) fn validate_template(json: &str) -> Result<Template, String> {
    if json.len() > 4096 {
        return Err("Person name template is too large".into());
    }
    let template: Template = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if template.kind != KIND || template.tags != vec![vec!["d", D_TAG]] {
        return Err("Not a person-name template".into());
    }
    content_name(&template.content)?;
    Ok(template)
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.chars().count() > 64 {
        return Err("Person name must have at most 64 characters".into());
    }
    Ok(())
}

fn content_name(json: &str) -> Result<String, String> {
    let content: Content = serde_json::from_str(json).map_err(|e| e.to_string())?;
    validate_name(&content.name)?;
    Ok(content.name)
}

pub(crate) fn verified_name(json: &str, person: &str) -> Option<String> {
    let event = DeviceIdentity::verify_event(json).ok()?;
    if event.author != person || event.kind != KIND || event.tags != vec![vec!["d", D_TAG]] {
        return None;
    }
    content_name(&event.content).ok()
}

/// Empty setup names mean the step was skipped. Both record kinds must be
/// verified before their timestamp or text can influence the result.
pub(crate) fn resolve(records: &[String], person: &str) -> Option<String> {
    let mut records: Vec<_> = records
        .iter()
        .filter_map(|json| {
            let event = DeviceIdentity::verify_event(json).ok()?;
            (event.author == person).then_some((event, json))
        })
        .collect();
    records.sort_by(|(a, _), (b, _)| b.created_at.cmp(&a.created_at).then(a.id.cmp(&b.id)));
    let setup = records
        .iter()
        .find_map(|(_, json)| verified_name(json, person));
    if let Some(name) = setup.filter(|name| !name.trim().is_empty()) {
        return Some(name);
    }
    let profile = records.iter().find(|(event, _)| event.kind == 0)?;
    let content: serde_json::Value = serde_json::from_str(&profile.0.content).ok()?;
    ["display_name", "name"].iter().find_map(|key| {
        content
            .get(key)?
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
    })
}

#[cfg(test)]
mod tests;
