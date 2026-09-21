use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

pub const INPUT_PROFILE_BUNDLE_PATH: &str = "share/korri-input-profile";
pub const INPUT_PROFILE_NAME: &str = "korri-60-xbox_one_gamepad.yaml";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Component {
    InputPlumber,
    Inputd,
    InputSeatReceiver,
    Korrid,
    LocalSigner,
}

impl Component {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "inputplumber" => Ok(Self::InputPlumber),
            "inputd" => Ok(Self::Inputd),
            "input-seat-receiver" => Ok(Self::InputSeatReceiver),
            "korrid" => Ok(Self::Korrid),
            "local-signer" => Ok(Self::LocalSigner),
            other => Err(format!(
                "component must be inputplumber, inputd, input-seat-receiver, korrid, or local-signer, got {other:?}"
            )),
        }
    }

    pub const fn executable(self) -> &'static str {
        match self {
            Self::InputPlumber => "bin/inputplumber",
            Self::Inputd => "bin/korri-inputd",
            Self::InputSeatReceiver => "bin/korri-input-seat-receiver",
            Self::Korrid => "bin/korrid",
            Self::LocalSigner => "bin/korri-local-signer",
        }
    }
}

pub fn resolve_bundle(selector: &Path, store_root: &Path) -> Result<PathBuf, String> {
    if !selector.is_absolute() {
        return Err("active bundle selector must be absolute".into());
    }
    let bundle = std::fs::canonicalize(selector)
        .map_err(|error| format!("active bundle is unavailable: {error}"))?;
    if !is_store_item_root(&bundle, store_root) || !bundle.is_dir() {
        return Err("active bundle must resolve to one immutable store directory".into());
    }
    for component in [
        Component::InputPlumber,
        Component::Inputd,
        Component::InputSeatReceiver,
        Component::Korrid,
        Component::LocalSigner,
    ] {
        resolve_component_in_bundle(&bundle, component, store_root)?;
    }
    let data = bundle.join("share/inputplumber");
    let data_link = std::fs::symlink_metadata(&data)
        .map_err(|error| format!("InputPlumber bundle data is unavailable: {error}"))?;
    if !data_link.file_type().is_symlink() {
        return Err("InputPlumber bundle data must be an immutable bundle symlink".into());
    }
    let data = std::fs::canonicalize(&data)
        .map_err(|error| format!("InputPlumber bundle data is unavailable: {error}"))?;
    if !is_inside_store_item(&data, store_root) || !data.is_dir() {
        return Err("InputPlumber bundle data must resolve inside the immutable store".into());
    }
    resolve_profile_in_bundle(&bundle, &data, store_root)?;
    Ok(bundle)
}

pub fn resolve_component(
    selector: &Path,
    component: Component,
    store_root: &Path,
) -> Result<PathBuf, String> {
    let bundle = resolve_bundle(selector, store_root)?;
    resolve_component_in_bundle(&bundle, component, store_root)
}

pub fn resolve_profile(selector: &Path, store_root: &Path) -> Result<PathBuf, String> {
    let bundle = resolve_bundle(selector, store_root)?;
    let data = std::fs::canonicalize(bundle.join("share/inputplumber"))
        .map_err(|error| format!("InputPlumber bundle data is unavailable: {error}"))?;
    resolve_profile_in_bundle(&bundle, &data, store_root)
}

fn resolve_profile_in_bundle(
    bundle: &Path,
    data: &Path,
    store_root: &Path,
) -> Result<PathBuf, String> {
    let selected = bundle.join(INPUT_PROFILE_BUNDLE_PATH);
    let link = std::fs::symlink_metadata(&selected)
        .map_err(|error| format!("selected input profile is unavailable: {error}"))?;
    if !link.file_type().is_symlink() {
        return Err("selected input profile must be an immutable bundle symlink".into());
    }
    let profile = std::fs::canonicalize(&selected)
        .map_err(|error| format!("selected input profile is unavailable: {error}"))?;
    if !is_inside_store_item(&profile, store_root)
        || !profile.is_file()
        || profile.file_name().and_then(|name| name.to_str()) != Some(INPUT_PROFILE_NAME)
        || store_item_root(&profile, store_root) != store_item_root(data, store_root)
    {
        return Err(
            "selected input profile must resolve inside the InputPlumber store item".into(),
        );
    }
    Ok(profile)
}

fn resolve_component_in_bundle(
    bundle: &Path,
    component: Component,
    store_root: &Path,
) -> Result<PathBuf, String> {
    let selected = bundle.join(component.executable());
    let link = std::fs::symlink_metadata(&selected)
        .map_err(|error| format!("selected component is unavailable: {error}"))?;
    if !link.file_type().is_symlink() {
        return Err("selected component must be an immutable bundle symlink".into());
    }
    let executable = std::fs::canonicalize(&selected)
        .map_err(|error| format!("selected component target is unavailable: {error}"))?;
    if !is_inside_store_item(&executable, store_root) {
        return Err("selected component must resolve inside the immutable store".into());
    }
    let metadata = std::fs::metadata(&executable)
        .map_err(|error| format!("selected component target is unavailable: {error}"))?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return Err("selected component target must be a regular executable".into());
    }
    Ok(executable)
}

fn store_item_root(path: &Path, store_root: &Path) -> Option<PathBuf> {
    let relative = path.strip_prefix(store_root).ok()?;
    let std::path::Component::Normal(item) = relative.components().next()? else {
        return None;
    };
    Some(store_root.join(item))
}

pub fn is_store_item_root(path: &Path, store_root: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(store_root) else {
        return false;
    };
    let mut components = relative.components();
    matches!(components.next(), Some(std::path::Component::Normal(item)) if !item.is_empty())
        && components.next().is_none()
}

pub fn is_inside_store_item(path: &Path, store_root: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(store_root) else {
        return false;
    };
    let mut components = relative.components();
    matches!(components.next(), Some(std::path::Component::Normal(item)) if !item.is_empty())
        && components.next().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let store = root.path().join("store");
        let bundle = store.join("bundle");
        let package = store.join("packages");
        std::fs::create_dir_all(bundle.join("bin")).unwrap();
        std::fs::create_dir_all(bundle.join("share")).unwrap();
        std::fs::create_dir_all(package.join("bin")).unwrap();
        std::fs::create_dir_all(package.join("share/inputplumber/profiles")).unwrap();
        for name in [
            "inputplumber",
            "korri-inputd",
            "korri-input-seat-receiver",
            "korrid",
            "korri-local-signer",
        ] {
            let executable = package.join("bin").join(name);
            std::fs::write(&executable, b"fixture").unwrap();
            let mut permissions = std::fs::metadata(&executable).unwrap().permissions();
            permissions.set_mode(0o555);
            std::fs::set_permissions(&executable, permissions).unwrap();
            symlink(&executable, bundle.join("bin").join(name)).unwrap();
        }
        symlink(
            package.join("share/inputplumber"),
            bundle.join("share/inputplumber"),
        )
        .unwrap();
        let profile = package
            .join("share/inputplumber/profiles")
            .join(INPUT_PROFILE_NAME);
        std::fs::write(&profile, b"profile").unwrap();
        symlink(&profile, bundle.join(INPUT_PROFILE_BUNDLE_PATH)).unwrap();
        let selector = root.path().join("active");
        symlink(&bundle, &selector).unwrap();
        (root, store, selector)
    }

    #[test]
    fn resolves_one_fixed_component_and_profile_from_an_immutable_bundle() {
        let (_root, store, selector) = fixture();

        let executable = resolve_component(&selector, Component::Inputd, &store).unwrap();
        let profile = resolve_profile(&selector, &store).unwrap();

        assert_eq!(
            executable.file_name().and_then(|name| name.to_str()),
            Some("korri-inputd")
        );
        assert_eq!(
            profile.file_name().and_then(|name| name.to_str()),
            Some(INPUT_PROFILE_NAME)
        );
    }

    #[test]
    fn rejects_a_regular_file_in_place_of_the_bundle_symlink() {
        let (root, store, selector) = fixture();
        let bundle = std::fs::canonicalize(&selector).unwrap();
        std::fs::remove_file(bundle.join("bin/korri-inputd")).unwrap();
        std::fs::write(bundle.join("bin/korri-inputd"), b"not a link").unwrap();

        let error = resolve_component(&selector, Component::Inputd, &store).unwrap_err();

        assert_eq!(
            error,
            "selected component must be an immutable bundle symlink"
        );
        drop(root);
    }

    #[test]
    fn rejects_mutable_data_in_place_of_the_bundle_symlink() {
        let (_root, store, selector) = fixture();
        let bundle = std::fs::canonicalize(&selector).unwrap();
        std::fs::remove_file(bundle.join("share/inputplumber")).unwrap();
        std::fs::create_dir(bundle.join("share/inputplumber")).unwrap();

        let error = resolve_bundle(&selector, &store).unwrap_err();

        assert_eq!(
            error,
            "InputPlumber bundle data must be an immutable bundle symlink"
        );
    }

    #[test]
    fn rejects_a_profile_from_another_store_item() {
        let (root, store, selector) = fixture();
        let bundle = std::fs::canonicalize(&selector).unwrap();
        let other = store.join("other").join(INPUT_PROFILE_NAME);
        std::fs::create_dir_all(other.parent().unwrap()).unwrap();
        std::fs::write(&other, b"other profile").unwrap();
        std::fs::remove_file(bundle.join(INPUT_PROFILE_BUNDLE_PATH)).unwrap();
        symlink(&other, bundle.join(INPUT_PROFILE_BUNDLE_PATH)).unwrap();

        let error = resolve_bundle(&selector, &store).unwrap_err();

        assert_eq!(
            error,
            "selected input profile must resolve inside the InputPlumber store item"
        );
        drop(root);
    }

    #[test]
    fn rejects_each_missing_executable_in_the_public_bundle_contract() {
        for component in [
            Component::InputPlumber,
            Component::Inputd,
            Component::InputSeatReceiver,
            Component::Korrid,
        ] {
            let (_root, store, selector) = fixture();
            let bundle = std::fs::canonicalize(&selector).unwrap();
            std::fs::remove_file(bundle.join(component.executable())).unwrap();

            assert!(resolve_bundle(&selector, &store).is_err(), "{component:?}");
        }
    }

    #[test]
    fn rejects_a_nonexecutable_component_target() {
        let (_root, store, selector) = fixture();
        let target = resolve_component(&selector, Component::Korrid, &store).unwrap();
        std::fs::set_permissions(target, std::fs::Permissions::from_mode(0o444)).unwrap();

        assert_eq!(
            resolve_bundle(&selector, &store).unwrap_err(),
            "selected component target must be a regular executable"
        );
    }

    #[test]
    fn rejects_data_outside_the_store() {
        let (root, store, selector) = fixture();
        let bundle = std::fs::canonicalize(&selector).unwrap();
        std::fs::remove_file(bundle.join("share/inputplumber")).unwrap();
        symlink(root.path(), bundle.join("share/inputplumber")).unwrap();

        assert_eq!(
            resolve_bundle(&selector, &store).unwrap_err(),
            "InputPlumber bundle data must resolve inside the immutable store"
        );
    }

    #[test]
    fn rejects_a_missing_profile_and_a_profile_with_the_wrong_name() {
        let (_root, store, selector) = fixture();
        let bundle = std::fs::canonicalize(&selector).unwrap();
        let selected = bundle.join(INPUT_PROFILE_BUNDLE_PATH);
        std::fs::remove_file(&selected).unwrap();
        assert!(resolve_bundle(&selector, &store).is_err());
        let profile = store.join("packages/share/inputplumber/profiles/wrong.yaml");
        std::fs::write(&profile, b"profile").unwrap();
        symlink(profile, selected).unwrap();

        assert_eq!(
            resolve_bundle(&selector, &store).unwrap_err(),
            "selected input profile must resolve inside the InputPlumber store item"
        );
    }

    #[test]
    fn rejects_a_store_subdirectory_as_the_bundle_root() {
        let (_root, store, selector) = fixture();
        let bundle = std::fs::canonicalize(&selector).unwrap();

        assert_eq!(
            resolve_bundle(&bundle.join("bin"), &store).unwrap_err(),
            "active bundle must resolve to one immutable store directory"
        );
    }

    #[test]
    fn rejects_a_selector_outside_the_store() {
        let root = tempfile::tempdir().unwrap();
        let selector = root.path().join("active");
        std::fs::create_dir(&selector).unwrap();

        let error =
            resolve_component(&selector, Component::Inputd, Path::new("/store")).unwrap_err();

        assert_eq!(
            error,
            "active bundle must resolve to one immutable store directory"
        );
    }
}
