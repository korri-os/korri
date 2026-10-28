//! Device-local pins (approved schema sheet row 9). Config bindings are separate.
use crate::{
    package,
    plugin_admin::{PluginPublisher, PluginPublisherState},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const DISCLOSURE: &str = "This plugin is not signed. Korri cannot confirm who made it, or that updates come from the same person. Install it only if you trust the address you got it from.";
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pin {
    pub public_key: String,
}
pub type Pins = BTreeMap<String, Pin>;

pub fn fingerprint(key: &str) -> Result<String, String> {
    package::public_key_bytes(key)?;
    let (label, encoded) = key.split_once(':').unwrap();
    Ok(format!(
        "{label} {}",
        encoded.as_bytes()[..16]
            .chunks(4)
            .map(|part| std::str::from_utf8(part).unwrap())
            .collect::<Vec<_>>()
            .join(" ")
    ))
}

pub fn publisher(
    namespace: &str,
    state: PluginPublisherState,
    key: Option<&str>,
) -> Result<PluginPublisher, String> {
    Ok(PluginPublisher {
        namespace: namespace.into(),
        state,
        fingerprint: key.map(fingerprint).transpose()?,
        previous_fingerprint: None,
        disclosure: (state == PluginPublisherState::Unsigned).then(|| DISCLOSURE.into()),
        public_key: None,
    })
}

pub fn inspect(
    namespace: &str,
    release_key: Option<&str>,
    bindings: &package::PublisherBindings,
    pins: &Pins,
) -> Result<PluginPublisher, String> {
    use PluginPublisherState::*;
    let bound = bindings.get(namespace).map(|b| b.public_key.as_str());
    let pinned = pins.get(namespace).map(|b| b.public_key.as_str());
    let Some(key) = release_key else {
        if bound.or(pinned).is_some() {
            return Err(format!(
                "namespace {namespace} is already trusted; unsigned plugins are refused"
            ));
        }
        return publisher(namespace, Unsigned, None);
    };
    package::public_key_bytes(key)?;
    if let Some(bound) = bound {
        if bound != key {
            return Err(format!(
                "package is not signed by the full key bound to publisher {namespace}"
            ));
        }
        return publisher(namespace, Bound, Some(key));
    }
    if pinned == Some(key) {
        return publisher(namespace, Trusted, Some(key));
    }
    let mut result = publisher(
        namespace,
        if pinned.is_some() { KeyChanged } else { New },
        Some(key),
    )?;
    result.previous_fingerprint = pinned.map(fingerprint).transpose()?;
    result.public_key = Some(key.into());
    Ok(result)
}

pub fn require_trusted(publisher: &PluginPublisher) -> Result<(), String> {
    use PluginPublisherState::*;
    match publisher.state {
        New => Err(format!(
            "publisher {} is not trusted on this device",
            publisher.namespace
        )),
        KeyChanged => Err(format!(
            "publisher {} key changed: was {}, now {}",
            publisher.namespace,
            publisher
                .previous_fingerprint
                .as_deref()
                .unwrap_or("unknown"),
            publisher.fingerprint.as_deref().unwrap_or("unknown")
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_trust_key_change_bound_and_unsigned_are_distinct() {
        let a = "label:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
        assert_eq!(fingerprint(a).unwrap(), "label AAAA AAAA AAAA AAAA");
        let b = "label:AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=";
        let bindings = BTreeMap::new();
        let mut pins = Pins::new();
        let new = inspect("@a", Some(a), &bindings, &pins).unwrap();
        assert_eq!(new.state, PluginPublisherState::New);
        assert!(require_trusted(&new).is_err());
        assert_eq!(new.public_key.as_deref(), Some(a));
        assert_eq!(
            inspect("@a", None, &bindings, &pins)
                .unwrap()
                .disclosure
                .as_deref(),
            Some(DISCLOSURE)
        );
        pins.insert(
            "@a".into(),
            Pin {
                public_key: a.into(),
            },
        );
        assert_eq!(
            inspect("@a", Some(a), &bindings, &pins).unwrap().state,
            PluginPublisherState::Trusted
        );
        assert!(inspect("@a", None, &bindings, &pins).is_err());
        let changed = inspect("@a", Some(b), &bindings, &pins).unwrap();
        assert_eq!(changed.state, PluginPublisherState::KeyChanged);
        assert!(require_trusted(&changed)
            .unwrap_err()
            .contains("key changed: was"));
        let bindings = BTreeMap::from([(
            "@a".into(),
            package::PublisherBinding {
                public_key: a.into(),
                cache_url: "https://cache.example".into(),
            },
        )]);
        assert_eq!(
            inspect("@a", Some(a), &bindings, &pins).unwrap().state,
            PluginPublisherState::Bound
        );
        assert!(inspect("@a", Some(b), &bindings, &pins).is_err());
        assert!(inspect("@a", None, &bindings, &Pins::new()).is_err());
    }
}
