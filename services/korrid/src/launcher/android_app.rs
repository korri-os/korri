//! Mapper from a resolved Android plugin route to the launcher-neutral Android
//! instruction consumed by the shell.
//!
//! The route resolver owns legacy readable selection. This module only accepts
//! the one first-party Android-app integration token and emits the existing
//! unsigned instruction shape; signing remains with the RPC boundary.

use super::{AndroidComponent, LaunchSpec};
use crate::config::resolver::ResolvedRoute;
use std::collections::HashMap;

const ANDROID_APP_PROVIDER: &str = "@korri:android-app";
const ANDROID_APP_RUNNER: &str = "@korri:android-app/android-app";
const ANDROID_SYSTEM: &str = "android";
const ANDROID_APP_TOKEN: &str = "android-app";
const TARGET_PREFIX: &str = "@korri:android-app:";

#[derive(Debug, thiserror::Error)]
pub enum AndroidAppRouteError {
    #[error("Android app route {playable_id} uses unsupported provider {provider_id}")]
    Provider {
        playable_id: String,
        provider_id: String,
    },
    #[error("Android app route {playable_id} uses unsupported runner {runner_id}")]
    Runner {
        playable_id: String,
        runner_id: String,
    },
    #[error("Android app route {playable_id} uses unsupported family {family_id:?}")]
    Family {
        playable_id: String,
        family_id: Option<String>,
    },
    #[error("Android app route {playable_id} uses unsupported system {system_id}")]
    System {
        playable_id: String,
        system_id: String,
    },
    #[error(
        "Android app route {playable_id} uses unsupported integration token {integration_token}"
    )]
    IntegrationToken {
        playable_id: String,
        integration_token: String,
    },
    #[error("Android app route {playable_id} target must start with {TARGET_PREFIX}")]
    TargetPrefix { playable_id: String },
    #[error("Android app route {playable_id} target has invalid package name")]
    PackageName { playable_id: String },
}

pub fn launch_route(route: &ResolvedRoute) -> Result<LaunchSpec, AndroidAppRouteError> {
    if route.provider_id != ANDROID_APP_PROVIDER {
        return Err(AndroidAppRouteError::Provider {
            playable_id: route.playable_id.clone(),
            provider_id: route.provider_id.clone(),
        });
    }
    if route.runner_id != ANDROID_APP_RUNNER {
        return Err(AndroidAppRouteError::Runner {
            playable_id: route.playable_id.clone(),
            runner_id: route.runner_id.clone(),
        });
    }
    if route.family_id.as_deref() != Some(ANDROID_APP_PROVIDER) {
        return Err(AndroidAppRouteError::Family {
            playable_id: route.playable_id.clone(),
            family_id: route.family_id.clone(),
        });
    }
    if route.system_id != ANDROID_SYSTEM {
        return Err(AndroidAppRouteError::System {
            playable_id: route.playable_id.clone(),
            system_id: route.system_id.clone(),
        });
    }
    if route.integration_token != ANDROID_APP_TOKEN {
        return Err(AndroidAppRouteError::IntegrationToken {
            playable_id: route.playable_id.clone(),
            integration_token: route.integration_token.clone(),
        });
    }
    let package_name = route
        .flattened_target
        .strip_prefix(TARGET_PREFIX)
        .ok_or_else(|| AndroidAppRouteError::TargetPrefix {
            playable_id: route.playable_id.clone(),
        })?;
    if package_name.is_empty()
        || package_name.contains(':')
        || package_name.contains('/')
        || package_name.chars().any(char::is_whitespace)
    {
        return Err(AndroidAppRouteError::PackageName {
            playable_id: route.playable_id.clone(),
        });
    }

    Ok(LaunchSpec {
        // The RPC preparation boundary replaces this before signing.
        launch_id: String::new(),
        runner_id: ANDROID_APP_TOKEN.into(),
        disposition: super::types::LaunchDisposition::Fresh,
        context: super::types::LaunchContext::unresolved(),
        component: AndroidComponent {
            package_name: package_name.into(),
            // Intentionally unused for android-app: Android package updates can
            // rename the launcher Activity, so the shell resolves the current
            // launcher intent with PackageManager at the moment of launch.
            class_name: String::new(),
        },
        extras: HashMap::new(),
        directories: Vec::new(),
        files: Vec::new(),
        authorized_content_root: None,
        integrity: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route() -> ResolvedRoute {
        ResolvedRoute {
            playable_id: "configured-game".into(),
            title: Some("Configured Game".into()),
            release_id: "android".into(),
            identity: None,
            provider_id: "@korri:android-app".into(),
            system_id: "android".into(),
            system_title: Some("Android".into()),
            runner_id: "@korri:android-app/android-app".into(),
            family_id: Some("@korri:android-app".into()),
            integration_token: "android-app".into(),
            flattened_target: "@korri:android-app:org.example.game".into(),
            android_component: None,
            linux_runner: None,
            core_path: None,
            file_target: None,
        }
    }

    #[test]
    fn maps_a_valid_resolved_route_to_the_existing_unsigned_android_shape() {
        let spec = launch_route(&route()).expect("Android route should map");

        assert_eq!(spec.runner_id, "android-app");
        assert_eq!(spec.component.package_name, "org.example.game");
        assert!(spec.component.class_name.is_empty());
        assert!(spec.extras.is_empty());
        assert!(spec.directories.is_empty());
        assert!(spec.files.is_empty());
        assert!(spec.integrity.is_empty());
    }

    #[test]
    fn rejects_routes_that_do_not_match_the_first_party_android_integration() {
        let mut unsupported_token = route();
        unsupported_token.integration_token = "sh".into();
        assert!(matches!(
            launch_route(&unsupported_token),
            Err(AndroidAppRouteError::IntegrationToken { .. })
        ));

        let mut wrong_prefix = route();
        wrong_prefix.flattened_target = "@korri:other:org.example.game".into();
        assert!(matches!(
            launch_route(&wrong_prefix),
            Err(AndroidAppRouteError::TargetPrefix { .. })
        ));
    }

    #[test]
    fn rejects_routes_with_wrong_android_identity_guards() {
        let cases: [(&str, Box<dyn FnOnce(&mut ResolvedRoute)>); 4] = [
            (
                "provider",
                Box::new(|route| route.provider_id = "@korri:other".into()),
            ),
            (
                "runner",
                Box::new(|route| route.runner_id = "@korri:android-app/other".into()),
            ),
            (
                "family",
                Box::new(|route| route.family_id = Some("@korri:other".into())),
            ),
            (
                "system",
                Box::new(|route| route.system_id = "switch".into()),
            ),
        ];

        for (label, mutate) in cases {
            let mut candidate = route();
            mutate(&mut candidate);
            match (label, launch_route(&candidate)) {
                ("provider", Err(AndroidAppRouteError::Provider { .. })) => {}
                ("runner", Err(AndroidAppRouteError::Runner { .. })) => {}
                ("family", Err(AndroidAppRouteError::Family { .. })) => {}
                ("system", Err(AndroidAppRouteError::System { .. })) => {}
                (other, result) => panic!("{other} guard returned {result:?}"),
            }
        }
    }
}
