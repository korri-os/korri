use korri_plugin_host::release::{batch_url, parse_paths};

const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";
const OUTPUT: &str = "/nix/store/0123456789abcdfghijklmnpqrsvwxyz-plugin";

#[test]
fn commit_lookup_uses_pages_metadata_and_the_publishers_batch_convention() {
    for cache in [
        "https://korri-os.github.io/plugins",
        "https://korri-os.github.io/plugins/",
    ] {
        assert_eq!(
            batch_url(cache, REVISION).unwrap().as_str(),
            "https://github.com/korri-os/plugins/releases/download/build-0123456789ab/"
        );
    }
    for revision in [
        "main",
        "latest",
        "build-0123456789ab",
        "0123456789ab",
        &"A".repeat(40),
        &"g".repeat(40),
    ] {
        assert!(
            batch_url("https://korri-os.github.io/plugins/", revision).is_err(),
            "{revision}"
        );
    }
    for (cache, owner, repo) in [
        (
            "https://miniarray.github.io/korri-plugins/",
            "miniarray",
            "korri-plugins",
        ),
        (
            "https://simonwjackson.github.io/korri-plugins/",
            "simonwjackson",
            "korri-plugins",
        ),
    ] {
        assert_eq!(
            batch_url(cache, REVISION).unwrap().as_str(),
            format!("https://github.com/{owner}/{repo}/releases/download/build-0123456789ab/")
        );
    }
    for cache in [
        "http://korri-os.github.io/plugins/",
        "https://korri-os.github.io.evil/plugins/",
        "https://nested.korri-os.github.io/plugins/",
        "https://user@korri-os.github.io/plugins/",
        "https://korri-os.github.io/plugins/?x=1",
        "https://korri-os.github.io/plugins/#fragment",
        "https://korri-os.github.io/plugins/extra",
        "https://korri-os.github.io/pl%75gins/",
        "https://korri-os.github.io/plugins//",
        "https://korri-os.github.io/plugins/../plugins/",
        "https://korri-os.github.io:443/plugins/",
        "https://korri-os.github.io:8443/plugins/",
        "https://-owner.github.io/plugins/",
        "https://owner-.github.io/plugins/",
        "https://korri-os.github.io/",
        "https://github.com/korri-os/plugins/releases/download/cache/",
        "http://github.com/korri-os/plugins/releases/download/cache/",
        "https://github.com.evil/korri-os/plugins/releases/download/cache/",
        "https://user@github.com/korri-os/plugins/releases/download/cache/",
        "https://github.com/korri-os/plugins/releases/download/cache/?x=1",
        "https://github.com/korri-os/plugins/releases/download/cache/#fragment",
        "https://github.com/korri-os/plugins/releases/download/cache/extra",
        "https://github.com/korri-os/plugins/releases/download/c%61che/",
        "https://github.com/korri-os/plugins/releases/download/build-0123456789ab/",
        "https://github.com/korri-os/plugins",
    ] {
        assert!(batch_url(cache, REVISION).is_err(), "{cache}");
    }
}

#[test]
fn path_evidence_is_exact_bounded_unique_output_lines() {
    let second = OUTPUT.replace("-plugin", "-second");
    let paths = parse_paths(&format!("{OUTPUT}\n{second}\n")).unwrap();
    assert_eq!(paths, vec![std::path::PathBuf::from(OUTPUT), second.into()]);
    for text in [
        String::new(),
        OUTPUT.into(),
        format!("{OUTPUT}\n\n"),
        format!("{OUTPUT}\r\n"),
        format!(" {OUTPUT}\n"),
        format!("{OUTPUT}\n{OUTPUT}\n"),
        format!("{OUTPUT}.drv\n"),
        format!("{OUTPUT}/plugin.ts\n"),
        format!("{OUTPUT}/\n"),
        format!("{OUTPUT}/../plugin\n"),
        ".#plugin\n".into(),
        format!("{OUTPUT}\0\n"),
        "x".repeat(64 * 1024 + 1),
    ] {
        assert!(parse_paths(&text).is_err(), "{text:?}");
    }
}
