//! TEMPORARY webOS input diagnostic. It records fixed labels only: whether SDL
//! reports keyboard focus on our window, and which kinds of focus and key
//! events SDL delivered. It never records key codes, text, pointers, errors,
//! arguments, environment, RPC data, the port or the capability.
//!
//! One private 0600 file per run, kept after exit for collection, at most
//! `LIMIT` lines. Root never writes one. A write failure disables the probe and
//! never changes what the application does.

use std::fs::File;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

const PREFIX: &str = "korri-slint-spike-input-";
pub const LIMIT: usize = 64;

/// Which window SDL says has keyboard focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    None,
    Own,
    Other,
}

/// Every line the probe can write. No variant carries runtime data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Record {
    Focus(Focus),
    KeyDown,
    KeyUp,
    FocusGained,
    FocusLost,
    AppIdInherited,
    AppIdSupplied,
}

impl Record {
    fn label(self) -> &'static str {
        match self {
            Record::Focus(Focus::None) => "keyboard_focus:none",
            Record::Focus(Focus::Own) => "keyboard_focus:own",
            Record::Focus(Focus::Other) => "keyboard_focus:other",
            Record::KeyDown => "event:key_down",
            Record::KeyUp => "event:key_up",
            Record::FocusGained => "event:focus_gained",
            Record::FocusLost => "event:focus_lost",
            Record::AppIdInherited => "env:appid_inherited",
            Record::AppIdSupplied => "env:appid_supplied",
        }
    }
}

pub struct InputProbe {
    file: Option<File>,
    written: usize,
    focus: Option<Focus>,
}

impl InputProbe {
    /// A probe that records nothing.
    pub fn disabled() -> Self {
        Self {
            file: None,
            written: 0,
            focus: None,
        }
    }

    /// The TV probe. Only without an X11 display, the same rule
    /// `prepare_environment` uses, so workstation checks leave no file.
    pub fn for_tv() -> Self {
        if std::env::var_os("DISPLAY").is_some() {
            return Self::disabled();
        }
        Self::open_in(Path::new("/tmp"))
    }

    fn open_in(directory: &Path) -> Self {
        // SAFETY: geteuid has no preconditions.
        if unsafe { libc::geteuid() } == 0 {
            return Self::disabled();
        }
        let file = tempfile::Builder::new()
            .prefix(PREFIX)
            .suffix(".log")
            .tempfile_in(directory)
            .ok()
            .and_then(|file| {
                file.as_file()
                    .set_permissions(std::fs::Permissions::from_mode(0o600))
                    .ok()?;
                file.keep().ok().map(|(file, _)| file)
            });
        Self {
            file,
            written: 0,
            focus: None,
        }
    }

    /// Record a focus sample only when it differs from the last one.
    pub fn focus(&mut self, focus: Focus) {
        if self.focus != Some(focus) {
            self.focus = Some(focus);
            self.record(Record::Focus(focus));
        }
    }

    pub fn record(&mut self, record: Record) {
        if self.written >= LIMIT {
            return;
        }
        let Some(file) = self.file.as_mut() else {
            return;
        };
        if writeln!(file, "{}", record.label()).is_err() {
            self.file = None;
            return;
        }
        self.written += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    const ALLOWED: [&str; 9] = [
        "keyboard_focus:none",
        "keyboard_focus:own",
        "keyboard_focus:other",
        "event:key_down",
        "event:key_up",
        "event:focus_gained",
        "event:focus_lost",
        "env:appid_inherited",
        "env:appid_supplied",
    ];

    fn files(directory: &Path) -> Vec<std::path::PathBuf> {
        std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect()
    }

    #[test]
    fn records_fixed_labels_privately_and_only_focus_changes() {
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let mut probe = InputProbe::open_in(directory.path());
        probe.focus(Focus::None);
        probe.focus(Focus::None);
        probe.record(Record::FocusGained);
        probe.focus(Focus::Own);
        probe.record(Record::KeyDown);
        probe.record(Record::KeyUp);
        probe.focus(Focus::Other);
        probe.record(Record::FocusLost);
        drop(probe);
        let paths = files(directory.path());
        assert_eq!(paths.len(), 1);
        let name = paths[0].file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with(PREFIX) && name.ends_with(".log"));
        let metadata = std::fs::symlink_metadata(&paths[0]).unwrap();
        assert!(metadata.is_file());
        assert_eq!(metadata.mode() & 0o777, 0o600);
        assert_eq!(
            std::fs::read_to_string(&paths[0]).unwrap(),
            "keyboard_focus:none\nevent:focus_gained\nkeyboard_focus:own\n\
             event:key_down\nevent:key_up\nkeyboard_focus:other\nevent:focus_lost\n"
        );
    }

    #[test]
    fn every_label_is_allowed_and_runs_are_unique() {
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        for _ in 0..2 {
            let mut probe = InputProbe::open_in(directory.path());
            for record in [
                Record::Focus(Focus::None),
                Record::Focus(Focus::Own),
                Record::Focus(Focus::Other),
                Record::KeyDown,
                Record::KeyUp,
                Record::FocusGained,
                Record::FocusLost,
                Record::AppIdInherited,
                Record::AppIdSupplied,
            ] {
                probe.record(record);
            }
        }
        let paths = files(directory.path());
        assert_eq!(paths.len(), 2);
        for path in paths {
            let text = std::fs::read_to_string(path).unwrap();
            assert_eq!(text.lines().collect::<Vec<_>>(), ALLOWED);
        }
    }

    #[test]
    fn stops_after_the_record_limit() {
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let mut probe = InputProbe::open_in(directory.path());
        for _ in 0..LIMIT + 10 {
            probe.record(Record::KeyDown);
        }
        drop(probe);
        let text = std::fs::read_to_string(&files(directory.path())[0]).unwrap();
        assert_eq!(text.lines().count(), LIMIT);
    }

    #[test]
    fn unwritable_directory_disables_without_failing() {
        let mut probe = InputProbe::open_in(Path::new("/nonexistent-korri-probe-directory"));
        probe.record(Record::KeyDown);
        probe.focus(Focus::Own);
    }
}
