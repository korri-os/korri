use super::*;
use crate::nixfree_install::tests::{fixture_with, manifest, nar_of, STORE_PATH};

const REPO: &str = "https://example.invalid/catalog.json";
const ARCHIVE: &str = "https://example.invalid/homebrewhub.tar";

fn admin(dir: &Path, archive: Vec<u8>, sha: &str, platform: &str) -> AndroidPluginAdmin {
    let store: &'static AndroidPluginStore = Box::leak(Box::new(AndroidPluginStore::new(dir)));
    let catalog = serde_json::json!({
        "records": [
            {"plugin_id":"@korri:homebrewhub","release_version":"r1","platform":"x86_64-linux",
             "store_path":STORE_PATH,"archive_url":"https://example.invalid/linux.tar","archive_sha256":"00"},
            {"plugin_id":"@korri:homebrewhub","title":"Homebrew Hub","release_version":"r1","platform":platform,
             "store_path":STORE_PATH,"archive_url":ARCHIVE,"archive_sha256":sha}
        ],
        "latest": {}
    })
    .to_string()
    .into_bytes();
    AndroidPluginAdmin::new(
        dir,
        store,
        None,
        Box::new(move |url, _| match url {
            REPO => Ok(catalog.clone()),
            ARCHIVE => Ok(archive.clone()),
            other => Err(format!("unexpected fetch {other}")),
        }),
    )
}

fn install(admin: &AndroidPluginAdmin, approval: &str) -> Result<PluginAdminResponse, RpcFailure> {
    admin.handle(PluginAdminRequest::Install(PluginInstallRequest {
        url: REPO.into(),
        plugin_id: "@korri:homebrewhub".into(),
        release_version: "r1".into(),
        approval: approval.into(),
    }))
}

#[test]
fn catalog_offers_only_android_releases_and_install_needs_trust_and_review() {
    let nar = nar_of(&[("manifest.json", &manifest("")), ("plugin.ts", b"x")]);
    let f = fixture_with(&nar, "", true);
    let dir = tempfile::tempdir().unwrap();
    let admin = admin(dir.path(), f.archive.clone(), &f.sha256, "any");

    // An unknown repository is refused.
    assert!(admin
        .handle(PluginAdminRequest::Catalog(PluginRepositoryRequest {
            url: REPO.into()
        }))
        .is_err());
    admin
        .handle(PluginAdminRequest::RepositoryAdd(PluginRepositoryRequest {
            url: REPO.into(),
        }))
        .unwrap();
    let PluginAdminResponse::Catalog(catalog) = admin
        .handle(PluginAdminRequest::Catalog(PluginRepositoryRequest {
            url: REPO.into(),
        }))
        .unwrap()
    else {
        panic!("wrong response")
    };
    assert_eq!(catalog.entries.len(), 1, "the Linux record is not offered");
    assert_eq!(
        catalog.entries[0].publisher.state,
        PluginPublisherState::New
    );

    // Untrusted publisher, then a stale approval, then success.
    assert_eq!(
        install(&admin, &f.sha256).unwrap_err().code,
        CODE_PUBLISHER_NOT_TRUSTED
    );
    admin
        .handle(PluginAdminRequest::TrustPublisher(
            PluginTrustPublisherRequest {
                namespace: "@korri".into(),
                public_key: f.key.clone(),
            },
        ))
        .unwrap();
    assert_eq!(
        install(&admin, "stale").unwrap_err().code,
        CODE_NEEDS_REVIEW
    );
    install(&admin, &f.sha256).unwrap();
    assert_eq!(admin.store.read().unwrap()[0].id, "@korri:homebrewhub");

    // Disable keeps the files; enable brings it back.
    for enabled in [false, true] {
        admin
            .handle(PluginAdminRequest::SetEnabled(PluginSetEnabledRequest {
                id: "@korri:homebrewhub".into(),
                enabled,
            }))
            .unwrap();
        assert_eq!(admin.store.read().unwrap().len(), usize::from(enabled));
    }
}

#[test]
fn a_rebound_namespace_key_is_refused() {
    let nar = nar_of(&[("manifest.json", &manifest("")), ("plugin.ts", b"x")]);
    let (a, b) = (fixture_with(&nar, "", true), fixture_with(&nar, "", true));
    let dir = tempfile::tempdir().unwrap();
    let admin = admin(dir.path(), a.archive.clone(), &a.sha256, "aarch64-android");
    let trust = |key: &str| {
        admin.handle(PluginAdminRequest::TrustPublisher(
            PluginTrustPublisherRequest {
                namespace: "@korri".into(),
                public_key: key.into(),
            },
        ))
    };
    trust(&a.key).unwrap();
    assert_eq!(trust(&b.key).unwrap_err().code, CODE_PUBLISHER_KEY_CHANGED);
}

/// End to end over HTTPS against a real served catalog (aka's tailnet test
/// catalog). Set KORRI_ANDROID_TEST_CATALOG and KORRI_ANDROID_TEST_KEY.
#[test]
fn installs_from_a_real_https_catalog_when_given() {
    let (Ok(url), Ok(key)) = (
        std::env::var("KORRI_ANDROID_TEST_CATALOG"),
        std::env::var("KORRI_ANDROID_TEST_KEY"),
    ) else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let store: &'static AndroidPluginStore = Box::leak(Box::new(AndroidPluginStore::new(dir.path())));
    let admin = AndroidPluginAdmin::new(dir.path(), store, Some(url.clone()), Box::new(https_fetch));
    for namespace in ["@korri", "@simonwjackson"] {
        admin
            .handle(PluginAdminRequest::TrustPublisher(PluginTrustPublisherRequest {
                namespace: namespace.into(),
                public_key: key.clone(),
            }))
            .unwrap();
    }
    for id in ["@korri:android-app", "@korri:homebrewhub", "@simonwjackson:savagewind"] {
        let PluginAdminResponse::Inspection(inspection) = admin
            .handle(PluginAdminRequest::Inspect(PluginInspectRequest {
                url: url.clone(),
                plugin_id: id.into(),
                release_version: "android-test-1".into(),
            }))
            .unwrap()
        else {
            panic!("wrong response")
        };
        admin
            .handle(PluginAdminRequest::Install(PluginInstallRequest {
                url: url.clone(),
                plugin_id: id.into(),
                release_version: "android-test-1".into(),
                approval: inspection.approval,
            }))
            .unwrap();
    }
    let enabled = store.read().unwrap();
    assert_eq!(enabled.len(), 3);
    // The installed packages load as a registry on an Android device.
    for package in &enabled {
        let source = crate::script::source::SourceSnapshot::package_plugin(
            &package.package,
            &package.entry,
            &package.sources,
        )
        .unwrap();
        let json = crate::script::eval_plugin_snapshot(&source).unwrap();
        let namespace = package.id.split_once(':').unwrap().0;
        crate::plugin::decode_plugin_declaration_on(namespace, &json, "aarch64-android").unwrap();
    }
}
