use crate::{config::ConfigSnapshot, relay::RelayError};

/// The same snapshot and JSON-array environment pattern as
/// relay.rs::RelayList::{from_device_settings,from_linux_environment}.
/// There is no built-in public STUN service. An empty list is valid.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct StunList(pub Vec<String>);
impl StunList {
    pub(crate) fn configured(values: Vec<String>) -> Result<Self, RelayError> {
        if values.len() > 8 || values.iter().any(|v| {
            v.is_empty() || v.len() > 253 || v.chars().any(char::is_whitespace)
                || v.contains(['/', '@', '?', '#'])
                || v.rsplit_once(':').is_none_or(|(host, port)| host.is_empty() || port.parse::<u16>().map_or(true, |p| p == 0))
        }) {
            return Err(RelayError::Configuration("STUN must contain at most 8 host:port addresses".into()));
        }
        Ok(Self(values))
    }
    pub(crate) fn from_linux_environment(value: Option<&str>) -> Result<Option<Self>, RelayError> {
        value.map(|json| serde_json::from_str::<Vec<String>>(json)
            .map_err(|_| RelayError::Configuration("KORRID_STUN must be a JSON array".into()))
            .and_then(Self::configured)).transpose()
    }
    pub(crate) fn from_device_settings(snapshot: &ConfigSnapshot) -> Result<Option<Self>, RelayError> {
        snapshot.host.as_ref().and_then(|h| h.stun.clone()).map(Self::configured).transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reach_stun_has_no_defaults_and_accepts_an_empty_list() {
        assert_eq!(StunList::from_linux_environment(None).unwrap(), None);
        assert_eq!(StunList::from_linux_environment(Some("[]")).unwrap(), Some(StunList::default()));
        assert!(StunList::configured(vec!["stun.invalid:3478".into()]).is_ok());
        assert!(StunList::configured(vec!["https://stun.invalid".into()]).is_err());
    }
}
