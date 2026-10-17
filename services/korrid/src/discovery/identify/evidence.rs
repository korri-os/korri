use super::{hash, EvidenceLevel};
use crate::plugin::ContentRules;

/// Rank shared by paired-source identification and the one-off migration.
pub fn level(
    name: &str,
    prefix: &[u8],
    rules: &ContentRules,
    hashes: &[&str],
    declared_systems: &dyn Fn(&str) -> Vec<String>,
) -> EvidenceLevel {
    if hashes
        .iter()
        .any(|hash| declared_systems(hash).contains(&rules.system))
    {
        EvidenceLevel::DeclaredHash
    } else if rules
        .evidence
        .bytes
        .iter()
        .any(|rule| hash::matches_bytes(prefix, rule))
    {
        EvidenceLevel::Bytes
    } else if rules
        .evidence
        .names
        .iter()
        .any(|pattern| regex::Regex::new(pattern).is_ok_and(|pattern| pattern.is_match(name)))
    {
        EvidenceLevel::Name
    } else {
        EvidenceLevel::Extension
    }
}
