//! Classifies the physical sources of one InputPlumber composite.
//!
//! A composite may merge several evdev sources on purpose. The Retroid Pocket
//! Mini V2 profile (`nix/devices/rpminiv2/inputplumber/devices/
//! 01-retroid-pocket-mini-v2.yaml`) merges its gamepad with two keyboard-only
//! volume-key devices. Two gamepad-class sources remain ambiguous, except for
//! the split-stick case below. The RG353M and RG DS profiles also merge an adc-keys
//! device that carries only the Fn key as `BTN_MODE`, which they map to Guide.
//! Split controllers may additionally have one keyless stick source with a
//! nonempty subset of ABS_X/Y/RX/RY. It may supplement one gamepad only when
//! their axes do not overlap. It cannot own a controller or reconnect identity.
//! The RG353M contributes all four axes; an X/Y-only split is also supported.
//!
//! Inputd runs unprivileged and must not open raw `/dev/input` nodes. It reads
//! the world-readable sysfs capability bitmaps instead:
//! `<sysfs>/class/input/eventN/device/capabilities/{key,abs}`.
//!
//! A source is known non-gamepad only when all of these are true:
//!
//! - its path is exactly `/dev/input/eventN`;
//! - both bitmaps are readable and well formed;
//! - the KEY bitmap sets no joystick or gamepad button: the `BTN_JOYSTICK`
//!   and `BTN_GAMEPAD` blocks (0x120-0x13f, which contains `BTN_SOUTH`
//!   0x130), `BTN_DPAD_UP` to `BTN_DPAD_RIGHT` (0x220-0x223), or
//!   `BTN_TRIGGER_HAPPY1` to `BTN_TRIGGER_HAPPY40` (0x2c0-0x2e7), with one
//!   exception: `BTN_MODE` (0x13c) does not count. A device with only that
//!   key is a lone Fn/Guide key, not a second gamepad. A gamepad that also
//!   reports `BTN_MODE` still sets other bits in these ranges;
//! - the ABS bitmap sets no axis at all.
//!
//! The keyless stick-axis sources above are classified separately. All other
//! sources with axes still count as gamepad-class, including touch devices.
//!
//! Zero gamepad-class sources means the gamepad is missing, even when
//! keyboard-only sources remain in the composite.
//!
//! This is a superset of the existing inputd rule (`devices.rs` treats KEY
//! 0x130 or 0x120 as a gamepad). It also follows InputPlumber 0.75.2
//! `EventDevice::get_driver_type`, which uses its keyboard driver only for
//! udev `ID_INPUT_KEYBOARD` devices that are not `ID_INPUT_JOYSTICK`, and falls
//! back to its gamepad driver for anything unknown. A non-evdev path or an
//! unreadable bitmap is unknown. Unknown sources preserve the old single-source
//! behavior, but a multi-source composite must classify every source. Otherwise
//! a disconnected gamepad plus readable volume keys could incorrectly be ready.
//!
//! The kernel prints each bitmap as hexadecimal `unsigned long` words, most
//! significant word first. Like `devices.rs`, this parser takes 64-bit words.

use std::path::PathBuf;

pub const SYSTEM_SYSFS_ROOT: &str = "/sys";

const BITS_PER_WORD: usize = 64;
const GAMEPAD_KEY_RANGES: [(usize, usize); 3] = [(0x120, 0x13f), (0x220, 0x223), (0x2c0, 0x2e7)];
const BTN_MODE: usize = 0x13c;
// ABS_X, ABS_Y, ABS_RX, ABS_RY. Other axes cannot be supplemental sticks.
const STICK_AXES_MASK: u64 = (1 << 0) | (1 << 1) | (1 << 3) | (1 << 4);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceClass {
    Gamepad,
    StickAxes,
    NonGamepad,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceTopology {
    MissingGamepad,
    OneGamepad,
    AmbiguousGamepads,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SysfsSourceClassifier {
    sysfs_root: PathBuf,
}

impl SysfsSourceClassifier {
    pub fn new(sysfs_root: impl Into<PathBuf>) -> Self {
        Self {
            sysfs_root: sysfs_root.into(),
        }
    }

    pub fn system() -> Self {
        Self::new(SYSTEM_SYSFS_ROOT)
    }

    pub fn classify(&self, source_path: &str) -> SourceClass {
        self.capabilities(source_path)
            .map_or(SourceClass::Unknown, |(class, _)| class)
    }

    /// `None` means the source cannot be classified.
    fn capabilities(&self, source_path: &str) -> Option<(SourceClass, u64)> {
        let event = source_path.strip_prefix("/dev/input/")?;
        let number = event.strip_prefix("event")?;
        if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let capabilities = self
            .sysfs_root
            .join("class/input")
            .join(event)
            .join("device/capabilities");
        let read = |name: &str| {
            std::fs::read_to_string(capabilities.join(name))
                .ok()
                .and_then(|content| parse_bitmap(&content))
        };
        let keys = read("key")?;
        let axes = read("abs")?;
        let has_gamepad_key = GAMEPAD_KEY_RANGES.iter().any(|&(first, last)| {
            (first..=last).any(|bit| bit != BTN_MODE && bitmap_has(&keys, bit))
        });
        let has_axis = axes.iter().any(|word| *word != 0);
        // Supplemental axes are all in the low word, so only that word can
        // overlap them even when the main pad advertises higher axis bits.
        let low_axes = axes[0];
        let class = if keys.iter().all(|word| *word == 0)
            && low_axes != 0
            && low_axes & !STICK_AXES_MASK == 0
            && axes.iter().skip(1).all(|word| *word == 0)
        {
            SourceClass::StickAxes
        } else if !has_gamepad_key && !has_axis {
            SourceClass::NonGamepad
        } else {
            SourceClass::Gamepad
        };
        Some((class, low_axes))
    }

    pub fn topology(&self, source_paths: &[String]) -> SourceTopology {
        let mut gamepads = 0;
        let mut sticks = 0;
        let mut gamepad_axes = 0;
        let mut stick_axes = 0;
        for path in source_paths {
            match self.capabilities(path) {
                Some((SourceClass::Gamepad, axes)) => {
                    gamepads += 1;
                    gamepad_axes |= axes;
                }
                Some((SourceClass::StickAxes, axes)) => {
                    sticks += 1;
                    stick_axes |= axes;
                }
                Some((SourceClass::NonGamepad, _)) => {}
                _ if source_paths.len() == 1 => gamepads += 1,
                _ => return SourceTopology::AmbiguousGamepads,
            }
        }
        if sticks > 1 || gamepad_axes & stick_axes != 0 {
            return SourceTopology::AmbiguousGamepads;
        }
        match gamepads {
            0 => SourceTopology::MissingGamepad,
            1 => SourceTopology::OneGamepad,
            _ => SourceTopology::AmbiguousGamepads,
        }
    }
}

/// Returns the words least significant first, or `None` when malformed.
fn parse_bitmap(content: &str) -> Option<Vec<u64>> {
    let mut words = content
        .split_whitespace()
        .map(|word| {
            if word.len() > BITS_PER_WORD / 4 || !word.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return None;
            }
            u64::from_str_radix(word, 16).ok()
        })
        .collect::<Option<Vec<_>>>()?;
    if words.is_empty() {
        return None;
    }
    words.reverse();
    Some(words)
}

fn bitmap_has(words: &[u64], bit: usize) -> bool {
    words
        .get(bit / BITS_PER_WORD)
        .is_some_and(|word| word & (1_u64 << (bit % BITS_PER_WORD)) != 0)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::{SourceClass, SourceTopology, SysfsSourceClassifier};

    // Observed on the Retroid Pocket Mini V2 through
    // /sys/class/input/eventN/device/capabilities/{key,abs}.
    const MINI_V2_GAMEPAD_KEY: &str = "f00000000 0 0 0 7cdb000000400000 0 0 0 0";
    const MINI_V2_GAMEPAD_ABS: &str = "30003f";
    const MINI_V2_GPIO_KEYS_KEY: &str = "8000000000000 0";
    const MINI_V2_RESIN_KEY: &str = "4000000000000 0";
    const NO_AXES: &str = "0";

    fn write_source(root: &Path, event: &str, key: Option<&str>, abs: Option<&str>) {
        let capabilities = root
            .join("class/input")
            .join(event)
            .join("device/capabilities");
        fs::create_dir_all(&capabilities).unwrap();
        if let Some(key) = key {
            fs::write(capabilities.join("key"), format!("{key}\n")).unwrap();
        }
        if let Some(abs) = abs {
            fs::write(capabilities.join("abs"), format!("{abs}\n")).unwrap();
        }
    }

    fn mini_v2_sysfs() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        write_source(
            root.path(),
            "event4",
            Some(MINI_V2_GAMEPAD_KEY),
            Some(MINI_V2_GAMEPAD_ABS),
        );
        write_source(
            root.path(),
            "event3",
            Some(MINI_V2_GPIO_KEYS_KEY),
            Some(NO_AXES),
        );
        write_source(
            root.path(),
            "event2",
            Some(MINI_V2_RESIN_KEY),
            Some(NO_AXES),
        );
        root
    }

    fn paths(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn bitmap_words_are_64_bit_most_significant_first() {
        let words = super::parse_bitmap("1 8000000000000001\n").unwrap();
        assert_eq!(words, vec![0x8000_0000_0000_0001, 1]);
        for bit in [0, 63, 64] {
            assert!(super::bitmap_has(&words, bit));
        }
        for bit in [1, 62, 65, 128] {
            assert!(!super::bitmap_has(&words, bit));
        }
    }

    #[test]
    fn mini_v2_gamepad_is_gamepad_class() {
        let root = mini_v2_sysfs();
        let classifier = SysfsSourceClassifier::new(root.path());
        assert_eq!(
            classifier.classify("/dev/input/event4"),
            SourceClass::Gamepad
        );
    }

    #[test]
    fn mini_v2_volume_key_sources_are_not_gamepad_class() {
        let root = mini_v2_sysfs();
        let classifier = SysfsSourceClassifier::new(root.path());
        assert_eq!(
            classifier.classify("/dev/input/event3"),
            SourceClass::NonGamepad
        );
        assert_eq!(
            classifier.classify("/dev/input/event2"),
            SourceClass::NonGamepad
        );
    }

    #[test]
    fn mini_v2_three_source_composite_has_one_gamepad() {
        let root = mini_v2_sysfs();
        let classifier = SysfsSourceClassifier::new(root.path());
        // Order observed through busctl SourceDevicePaths on the device.
        assert_eq!(
            classifier.topology(&paths(&[
                "/dev/input/event3",
                "/dev/input/event4",
                "/dev/input/event2",
            ])),
            SourceTopology::OneGamepad
        );
    }

    #[test]
    fn volume_keys_without_the_gamepad_are_a_missing_gamepad() {
        let root = mini_v2_sysfs();
        let classifier = SysfsSourceClassifier::new(root.path());
        assert_eq!(
            classifier.topology(&paths(&["/dev/input/event3", "/dev/input/event2"])),
            SourceTopology::MissingGamepad
        );
        assert_eq!(classifier.topology(&[]), SourceTopology::MissingGamepad);
    }

    #[test]
    fn two_gamepad_class_sources_are_ambiguous_even_beside_volume_keys() {
        let root = mini_v2_sysfs();
        write_source(
            root.path(),
            "event7",
            Some(MINI_V2_GAMEPAD_KEY),
            Some(MINI_V2_GAMEPAD_ABS),
        );
        let classifier = SysfsSourceClassifier::new(root.path());
        assert_eq!(
            classifier.topology(&paths(&[
                "/dev/input/event3",
                "/dev/input/event4",
                "/dev/input/event2",
                "/dev/input/event7",
            ])),
            SourceTopology::AmbiguousGamepads
        );
    }

    #[test]
    fn gamepad_buttons_without_axes_are_gamepad_class() {
        let root = tempfile::tempdir().unwrap();
        // BTN_SOUTH (0x130) only.
        write_source(
            root.path(),
            "event5",
            Some("1000000000000 0 0 0 0"),
            Some(NO_AXES),
        );
        // BTN_JOYSTICK (0x120) only.
        write_source(
            root.path(),
            "event6",
            Some("100000000 0 0 0 0"),
            Some(NO_AXES),
        );
        // BTN_DPAD_UP (0x220) only.
        write_source(
            root.path(),
            "event8",
            Some("100000000 0 0 0 0 0 0 0 0"),
            Some(NO_AXES),
        );
        // BTN_TRIGGER_HAPPY1 (0x2c0) only.
        write_source(
            root.path(),
            "event9",
            Some("1 0 0 0 0 0 0 0 0 0 0 0"),
            Some(NO_AXES),
        );
        let classifier = SysfsSourceClassifier::new(root.path());
        for event in ["event5", "event6", "event8", "event9"] {
            assert_eq!(
                classifier.classify(&format!("/dev/input/{event}")),
                SourceClass::Gamepad,
                "{event}"
            );
        }
    }

    // BTN_MODE (0x13c) only: the RG353M `adc-keys` and RG DS
    // `adc-keys-home` devices in their upstream device trees.
    const LONE_MODE_KEY: &str = "1000000000000000 0 0 0 0";

    #[test]
    fn a_lone_mode_key_is_not_gamepad_class() {
        let root = mini_v2_sysfs();
        write_source(root.path(), "event5", Some(LONE_MODE_KEY), Some(NO_AXES));
        // BTN_MODE beside BTN_SOUTH is still a gamepad.
        write_source(
            root.path(),
            "event6",
            Some("1001000000000000 0 0 0 0"),
            Some(NO_AXES),
        );
        let classifier = SysfsSourceClassifier::new(root.path());
        assert_eq!(
            classifier.classify("/dev/input/event5"),
            SourceClass::NonGamepad
        );
        assert_eq!(
            classifier.classify("/dev/input/event6"),
            SourceClass::Gamepad
        );
        assert_eq!(
            classifier.topology(&paths(&[
                "/dev/input/event4",
                "/dev/input/event3",
                "/dev/input/event5",
            ])),
            SourceTopology::OneGamepad
        );
        assert_eq!(
            classifier.topology(&paths(&["/dev/input/event3", "/dev/input/event5"])),
            SourceTopology::MissingGamepad
        );
    }

    #[test]
    fn rg353m_button_pad_and_separate_sticks_form_one_controller() {
        let root = tempfile::tempdir().unwrap();
        // Observed on haku, 2026-11-14. adc-joystick has no keys and ABS=1b.
        write_source(
            root.path(),
            "event0",
            Some("f00000000 0 0 0 6fdb000000000000 0 0 0 0"),
            Some("0"),
        );
        write_source(root.path(), "event5", Some("0"), Some("1b"));
        write_source(root.path(), "event1", Some("c000000000000 0"), Some("0"));
        write_source(root.path(), "event4", Some(LONE_MODE_KEY), Some("0"));
        let classifier = SysfsSourceClassifier::new(root.path());
        assert_eq!(
            classifier.topology(&paths(&[
                "/dev/input/event0",
                "/dev/input/event5",
                "/dev/input/event1",
                "/dev/input/event4"
            ])),
            SourceTopology::OneGamepad
        );
        // The button pad owns the reconnect identity; sticks cannot replace it.
        assert_eq!(
            classifier.topology(&paths(&[
                "/dev/input/event5",
                "/dev/input/event1",
                "/dev/input/event4"
            ])),
            SourceTopology::MissingGamepad
        );
        assert_eq!(
            classifier.topology(&paths(&["/dev/input/event5"])),
            SourceTopology::MissingGamepad
        );
        // Reject a second stick unit and any axes overlapping the main pad.
        write_source(root.path(), "event6", Some("0"), Some("1b"));
        assert_eq!(
            classifier.topology(&paths(&[
                "/dev/input/event0",
                "/dev/input/event5",
                "/dev/input/event6"
            ])),
            SourceTopology::AmbiguousGamepads
        );
        for abs in ["1", "1b", "30003f"] {
            write_source(
                root.path(),
                "event0",
                Some("1000000000000 0 0 0 0"),
                Some(abs),
            );
            assert_eq!(
                classifier.topology(&paths(&["/dev/input/event0", "/dev/input/event5"])),
                SourceTopology::AmbiguousGamepads
            );
        }
    }

    #[test]
    fn split_sticks_allow_nonempty_axis_subsets_and_disjoint_pad_axes() {
        let root = tempfile::tempdir().unwrap();
        // Main-pad triggers and hats do not overlap any stick axis.
        const PAD_AXES: u64 = 0x30024;
        write_source(
            root.path(),
            "event0",
            Some("1000000000000 0 0 0 0"),
            Some(&format!("{PAD_AXES:x}")),
        );
        let classifier = SysfsSourceClassifier::new(root.path());
        for axes in (1_u64..=0x1b).filter(|axes| axes & !0x1b == 0) {
            write_source(root.path(), "event5", Some("0"), Some(&format!("{axes:x}")));
            assert_eq!(
                classifier.classify("/dev/input/event5"),
                SourceClass::StickAxes
            );
            assert_eq!(
                classifier.topology(&paths(&["/dev/input/event0", "/dev/input/event5"])),
                SourceTopology::OneGamepad,
                "axes={axes:x}"
            );
            assert_eq!(
                classifier.topology(&paths(&["/dev/input/event5"])),
                SourceTopology::MissingGamepad
            );
            // Even one shared axis is ambiguous; other main-pad axes are fine.
            let shared = 1 << axes.trailing_zeros();
            write_source(
                root.path(),
                "event0",
                Some("1000000000000 0 0 0 0"),
                Some(&format!("{:x}", PAD_AXES | shared)),
            );
            assert_eq!(
                classifier.topology(&paths(&["/dev/input/event0", "/dev/input/event5"])),
                SourceTopology::AmbiguousGamepads,
                "overlap={shared:x}"
            );
            write_source(
                root.path(),
                "event0",
                Some("1000000000000 0 0 0 0"),
                Some(&format!("{PAD_AXES:x}")),
            );
        }
    }

    #[test]
    fn xy_only_split_shape_for_rg_ds_is_supported_without_enabling_its_profile() {
        let root = tempfile::tempdir().unwrap();
        write_source(
            root.path(),
            "event0",
            Some("1000000000000 0 0 0 0"),
            Some("0"),
        );
        // Candidate X/Y-only RG DS shape; no hardware claim or profile change.
        write_source(root.path(), "event5", Some("0"), Some("3"));
        let classifier = SysfsSourceClassifier::new(root.path());
        assert_eq!(
            classifier.topology(&paths(&["/dev/input/event0", "/dev/input/event5"])),
            SourceTopology::OneGamepad
        );
        // A second unit is still ambiguous even if its axes are disjoint.
        write_source(root.path(), "event6", Some("0"), Some("18"));
        assert_eq!(
            classifier.topology(&paths(&[
                "/dev/input/event0",
                "/dev/input/event5",
                "/dev/input/event6"
            ])),
            SourceTopology::AmbiguousGamepads
        );
    }

    #[test]
    fn supplemental_sticks_must_have_only_stick_axes_and_no_keys() {
        let root = tempfile::tempdir().unwrap();
        write_source(
            root.path(),
            "event0",
            Some("1000000000000 0 0 0 0"),
            Some("0"),
        );
        let classifier = SysfsSourceClassifier::new(root.path());
        for (key, abs) in [
            ("1", "1b"),
            (LONE_MODE_KEY, "1b"),
            ("0", "4"),
            ("0", "3f"),
            ("0", "3001b"),
            ("0", "1 1b"),
        ] {
            write_source(root.path(), "event5", Some(key), Some(abs));
            assert_eq!(
                classifier.topology(&paths(&["/dev/input/event0", "/dev/input/event5"])),
                SourceTopology::AmbiguousGamepads,
                "key={key} abs={abs}"
            );
        }
    }

    #[test]
    fn absolute_axes_without_gamepad_buttons_are_gamepad_class() {
        let root = tempfile::tempdir().unwrap();
        write_source(
            root.path(),
            "event5",
            Some(MINI_V2_GPIO_KEYS_KEY),
            Some("30000"),
        );
        let classifier = SysfsSourceClassifier::new(root.path());
        assert_eq!(
            classifier.classify("/dev/input/event5"),
            SourceClass::Gamepad
        );
    }

    #[test]
    fn unreadable_or_malformed_capabilities_are_unknown() {
        let root = tempfile::tempdir().unwrap();
        write_source(root.path(), "event1", None, Some(NO_AXES));
        write_source(root.path(), "event2", Some(MINI_V2_RESIN_KEY), None);
        write_source(root.path(), "event3", Some("not-hex 0"), Some(NO_AXES));
        write_source(root.path(), "event4", Some(MINI_V2_RESIN_KEY), Some("zz"));
        write_source(root.path(), "event5", Some(""), Some(NO_AXES));
        write_source(
            root.path(),
            "event6",
            Some("10000000000000000 0"),
            Some(NO_AXES),
        );
        let classifier = SysfsSourceClassifier::new(root.path());
        for event in [
            "event1", "event2", "event3", "event4", "event5", "event6", "event9",
        ] {
            assert_eq!(
                classifier.classify(&format!("/dev/input/{event}")),
                SourceClass::Unknown,
                "{event}"
            );
        }
    }

    #[test]
    fn source_paths_outside_dev_input_event_nodes_are_unknown() {
        let root = mini_v2_sysfs();
        let classifier = SysfsSourceClassifier::new(root.path());
        for path in [
            "/dev/hidraw0",
            "/dev/input/event",
            "/dev/input/event3x",
            "/dev/input/event3/../event2",
            "/dev/input/event3/",
            "/dev/input/event-3",
            "/dev/input/../input/event3",
            "/dev/inputplumber/sources/event3",
            "event3",
            "",
        ] {
            assert_eq!(classifier.classify(path), SourceClass::Unknown, "{path}");
        }
    }

    #[test]
    fn unknown_sources_only_preserve_the_old_single_source_behavior() {
        let root = mini_v2_sysfs();
        let classifier = SysfsSourceClassifier::new(root.path());
        assert_eq!(
            classifier.topology(&paths(&["/dev/hidraw0"])),
            SourceTopology::OneGamepad
        );
        assert_eq!(
            classifier.topology(&paths(&["/dev/hidraw0", "/dev/input/event3"])),
            SourceTopology::AmbiguousGamepads
        );
        // InputPlumber can still report the old paths while sysfs has already
        // removed the disconnected gamepad. Volume keys must not make it ready.
        fs::remove_dir_all(root.path().join("class/input/event4")).unwrap();
        assert_eq!(
            classifier.topology(&paths(&[
                "/dev/input/event3",
                "/dev/input/event4",
                "/dev/input/event2",
            ])),
            SourceTopology::AmbiguousGamepads
        );
    }

    #[test]
    fn one_unclassifiable_source_beside_the_gamepad_is_ambiguous() {
        let root = mini_v2_sysfs();
        let classifier = SysfsSourceClassifier::new(root.path());
        assert_eq!(
            classifier.topology(&paths(&["/dev/input/event4", "/dev/hidraw0"])),
            SourceTopology::AmbiguousGamepads
        );
    }
}
