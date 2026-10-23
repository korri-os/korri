//! ADR 0030: korrid's own archive reader for store downloads. korrid runs it
//! as `korrid unpack-archive <work-dir> <file>` in the ADR 0012 sandbox unit,
//! with the ADR 0012 unpack-program contract: exit 0 for success and 1 for
//! failure, diagnostics on stderr, no stdout contract, no rename of the work
//! folder. korrid identifies, publishes and records after the program exits.
//!
//! The reader checks every entry before it writes one, and one bad entry
//! refuses the whole archive. The rules come from Mega Man Rock N Roll's
//! `unpack.py`, legacy `safeZipPath` and the `O_NOFOLLOW` staging file: no
//! link, device, FIFO, socket or encrypted entry; no absolute path, drive
//! letter, `..`, `.` or empty segment, backslash, NUL or other control
//! character; no repeated path. Entry count and depth use discovery's
//! `TraversalBudget`. Files are written `0644` and folders `0755`, counted
//! against the free-space floor. A 7z or rar dictionary above `MemAvailable`
//! is refused before decoding.
use super::{
    space::{self, Floor, WRITE_CHUNK},
    Failure,
};
use crate::discovery::scanner::TraversalBudget;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, BufReader, Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

/// The legacy `ARTIFACT_MAGIC` rows for the three formats in scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Format {
    Zip,
    SevenZ,
    Rar,
}

/// The format of a download, from its first bytes. The name alone is not
/// enough: romhustler sends 7z and rar bytes under a `.zip` name.
pub(super) fn format(lead: &[u8]) -> Option<Format> {
    if [&b"PK\x03\x04"[..], b"PK\x05\x06", b"PK\x07\x08"]
        .iter()
        .any(|magic| lead.starts_with(magic))
    {
        Some(Format::Zip)
    } else if lead.starts_with(b"7z\xbc\xaf\x27\x1c") {
        Some(Format::SevenZ)
    } else if lead.starts_with(b"Rar!\x1a\x07") {
        Some(Format::Rar)
    } else {
        None
    }
}

/// True when a name ends in an archive extension that the store accepts
/// before the download.
pub(super) fn archive_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".zip", ".7z", ".rar"]
        .iter()
        .any(|suffix| lower.ends_with(suffix))
}

/// The device bounds one extraction runs under.
#[derive(Clone)]
pub(super) struct Limits {
    pub budget: TraversalBudget,
    /// `MemAvailable` from `/proc/meminfo`, in bytes, read when the reader starts.
    pub memory: u64,
    /// Test-only file system space.
    pub space: Option<space::Probe>,
    /// Set by korrid's in-process test reader on cancel. The sandbox unit is
    /// stopped instead.
    pub cancel: Option<Arc<AtomicBool>>,
}

impl Limits {
    pub(super) fn device() -> Result<Self, Stop> {
        Ok(Self {
            budget: TraversalBudget::default(),
            memory: mem_available(
                &fs::read_to_string("/proc/meminfo")
                    .map_err(|error| Stop::unpack(format!("cannot read /proc/meminfo: {error}")))?,
            )?,
            space: None,
            cancel: None,
        })
    }
}

/// `MemAvailable:  <n> kB` in bytes.
fn mem_available(meminfo: &str) -> Result<u64, Stop> {
    meminfo
        .lines()
        .find_map(|line| line.strip_prefix("MemAvailable:"))
        .and_then(|rest| rest.trim().strip_suffix("kB"))
        .and_then(|kib| kib.trim().parse::<u64>().ok())
        .map(|kib| kib.saturating_mul(1024))
        .ok_or_else(|| Stop::unpack("/proc/meminfo has no MemAvailable".into()))
}

/// Why the reader stopped. `reason` is a diagnostic for stderr only.
#[derive(Debug)]
pub(super) struct Stop {
    pub failure: Failure,
    pub reason: String,
}
impl Stop {
    fn unpack(reason: String) -> Self {
        Self {
            failure: Failure::Unpack,
            reason,
        }
    }
    fn refuse(reason: &str, name: &str) -> Self {
        Self::unpack(format!("{reason}: {name:?}"))
    }
}
impl From<Failure> for Stop {
    fn from(failure: Failure) -> Self {
        Self {
            failure,
            reason: format!("{failure:?}"),
        }
    }
}
impl From<io::Error> for Stop {
    fn from(error: io::Error) -> Self {
        let reason = error.to_string();
        Self {
            failure: error.into(),
            reason,
        }
    }
}

/// `korrid unpack-archive <work-dir> <file>`.
pub fn run(arguments: &[OsString]) -> Result<(), String> {
    let [work, file] = arguments else {
        return Err("usage: korrid unpack-archive WORK_DIR FILE".into());
    };
    let limits = Limits::device().map_err(|stop| stop.reason)?;
    extract(Path::new(file), Path::new(work), &limits)
        .map_err(|stop| format!("{:?}: {}", stop.failure, stop.reason))
}

/// Unpack `archive` into the empty folder `work`, which korrid made.
pub(super) fn extract(archive: &Path, work: &Path, limits: &Limits) -> Result<(), Stop> {
    let mut lead = [0u8; 8];
    let read = File::open(archive)?.read(&mut lead)?;
    let format = format(&lead[..read]).ok_or_else(|| Stop::unpack("not an archive".into()))?;
    if fs::read_dir(work)?.next().is_some() {
        return Err(Stop::unpack("work folder is not empty".into()));
    }
    let floor = Floor::new(work, limits.space.clone())?;
    let output = Rc::new(RefCell::new(Shared {
        floor,
        failure: None,
        cancel: limits.cancel.clone(),
    }));
    match format {
        Format::Zip => zip_archive(archive, work, limits, &output),
        Format::SevenZ => seven_z(archive, work, limits, &output),
        Format::Rar => rar(archive, work, limits, &output),
    }
}

#[derive(Debug)]
enum Kind {
    File { size: u64 },
    Folder,
}

#[derive(Debug)]
struct Entry {
    path: Vec<String>,
    kind: Kind,
}

/// The checked paths of every entry, before anything is written.
#[derive(Debug, Default)]
struct Plan {
    folders: BTreeSet<Vec<String>>,
    files: BTreeMap<Vec<String>, u64>,
}

/// Split and check one entry name. `legacy_separator` is the RAR 1.3 to 4.x
/// convention, whose headers use `\` as the folder separator.
fn segments(name: &str, folder: bool, legacy_separator: bool) -> Result<Vec<String>, Stop> {
    let converted;
    let mut path = name;
    if legacy_separator {
        converted = name.replace('\\', "/");
        path = &converted;
    }
    if folder {
        path = path.strip_suffix('/').unwrap_or(path);
    }
    if path.is_empty() || path.starts_with('/') {
        return Err(Stop::refuse("absolute or empty path", name));
    }
    let parts: Vec<String> = path.split('/').map(str::to_owned).collect();
    if let [first, ..] = parts.as_slice() {
        let bytes = first.as_bytes();
        if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            return Err(Stop::refuse("drive letter", name));
        }
    }
    for part in &parts {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.contains('\\')
            || part.chars().any(char::is_control)
            || !super::claims::safe_file_name(part)
        {
            return Err(Stop::refuse("unsafe path segment", name));
        }
    }
    Ok(parts)
}

fn plan(entries: Vec<Entry>, budget: &TraversalBudget) -> Result<Plan, Stop> {
    let mut plan = Plan::default();
    let mut seen = BTreeSet::new();
    let mut implied = BTreeSet::new();
    for entry in entries {
        let shown = entry.path.join("/");
        if !seen.insert(entry.path.clone()) {
            return Err(Stop::refuse("repeated path", &shown));
        }
        for end in 1..entry.path.len() {
            implied.insert(entry.path[..end].to_vec());
        }
        match entry.kind {
            Kind::Folder => {
                plan.folders.insert(entry.path);
            }
            Kind::File { size } => {
                plan.files.insert(entry.path, size);
            }
        }
    }
    if let Some(path) = plan.files.keys().find(|path| implied.contains(*path)) {
        return Err(Stop::refuse("a file is also a folder", &path.join("/")));
    }
    plan.folders.extend(implied);
    // Discovery's traversal budget: identification cannot see past it, so an
    // archive above it can never become a release.
    let entries = plan.folders.len() + plan.files.len();
    let mut per_folder: BTreeMap<&[String], usize> = BTreeMap::new();
    for path in plan.folders.iter().chain(plan.files.keys()) {
        *per_folder.entry(&path[..path.len() - 1]).or_default() += 1;
    }
    if entries > budget.max_entries
        || plan.folders.len() + 1 > budget.max_directories
        || plan
            .folders
            .iter()
            .any(|path| path.len() > budget.max_depth)
        || per_folder
            .values()
            .any(|count| *count > budget.max_sortable_entries)
    {
        return Err(Stop::unpack(
            "archive exceeds discovery's traversal budget".into(),
        ));
    }
    Ok(plan)
}

fn create_folders(work: &Path, plan: &Plan) -> Result<(), Stop> {
    // Sorted, so every parent exists before its children.
    for folder in &plan.folders {
        let path = join(work, folder);
        fs::DirBuilder::new().mode(0o755).create(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

fn join(work: &Path, path: &[String]) -> PathBuf {
    path.iter()
        .fold(work.to_path_buf(), |path, part| path.join(part))
}

struct Shared {
    floor: Floor,
    failure: Option<Stop>,
    cancel: Option<Arc<AtomicBool>>,
}

/// One file being written. Every write is at most `WRITE_CHUNK` bytes and is
/// counted against the floor; more bytes than the entry declares is a
/// corrupt archive.
struct Output {
    file: File,
    shared: Rc<RefCell<Shared>>,
    declared: u64,
    written: u64,
}

impl Output {
    fn create(
        work: &Path,
        path: &[String],
        declared: u64,
        shared: &Rc<RefCell<Shared>>,
    ) -> Result<Self, Stop> {
        let path = join(work, path);
        // O_EXCL: a name that collides on a case-insensitive file system fails
        // instead of overwriting.
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644))?;
        Ok(Self {
            file,
            shared: shared.clone(),
            declared,
            written: 0,
        })
    }

    fn fail(&self, stop: Stop) -> io::Error {
        let error = io::Error::other(stop.reason.clone());
        self.shared.borrow_mut().failure.get_or_insert(stop);
        error
    }

    fn finish(self) -> Result<(), Stop> {
        if self.written != self.declared {
            return Err(Stop::unpack(format!(
                "entry has {} bytes, declared {}",
                self.written, self.declared
            )));
        }
        Ok(())
    }
}

impl Write for Output {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let chunk = &buffer[..buffer.len().min(WRITE_CHUNK)];
        let cancelled = self
            .shared
            .borrow()
            .cancel
            .as_ref()
            .is_some_and(|cancel| cancel.load(Ordering::SeqCst));
        if cancelled {
            return Err(self.fail(Failure::Cancelled.into()));
        }
        if self.written + chunk.len() as u64 > self.declared {
            return Err(self.fail(Stop::unpack("entry is longer than declared".into())));
        }
        let reserved = self.shared.borrow_mut().floor.reserve(chunk.len() as u64);
        if let Err(failure) = reserved {
            return Err(self.fail(failure.into()));
        }
        self.file.write_all(chunk)?;
        self.written += chunk.len() as u64;
        Ok(chunk.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// The failure an Output recorded wins over the reader's own error.
fn recorded(shared: &Rc<RefCell<Shared>>, fallback: impl FnOnce() -> Stop) -> Stop {
    shared.borrow_mut().failure.take().unwrap_or_else(fallback)
}

/// Unix `st_mode` file-type bits: only regular files and folders are safe.
fn unix_type_is_safe(mode: u32, folder: bool) -> bool {
    match mode & 0o170000 {
        0 => true,
        0o100000 => !folder,
        0o040000 => folder,
        _ => false,
    }
}

fn zip_archive(
    archive: &Path,
    work: &Path,
    limits: &Limits,
    shared: &Rc<RefCell<Shared>>,
) -> Result<(), Stop> {
    let unreadable = |error: zip::result::ZipError| Stop::unpack(format!("zip: {error}"));
    let mut file = File::open(archive)?;
    let names = zip_central_names(&mut file)?;
    let mut zip = zip::ZipArchive::new(BufReader::new(file)).map_err(unreadable)?;
    // The reader keeps one entry per raw name, so a repeated name shows only
    // as fewer entries than korrid walked.
    if names.iter().collect::<BTreeSet<_>>().len() != names.len() {
        return Err(Stop::unpack("zip: repeated entry name".into()));
    }
    if zip.len() != names.len() {
        return Err(Stop::unpack(
            "zip: the reader lists other entries than the directory".into(),
        ));
    }
    let mut entries = Vec::with_capacity(zip.len().min(limits.budget.max_entries + 1));
    for index in 0..zip.len() {
        if entries.len() > limits.budget.max_entries {
            return Err(Stop::unpack(
                "archive exceeds discovery's traversal budget".into(),
            ));
        }
        let entry = zip.by_index_raw(index).map_err(unreadable)?;
        let name = entry.name().map_err(unreadable)?.into_owned();
        let folder = entry.is_dir();
        if entry.encrypted() {
            return Err(Stop::refuse("encrypted entry", &name));
        }
        if entry.is_symlink()
            || !entry
                .unix_mode()
                .is_none_or(|mode| unix_type_is_safe(mode, folder))
        {
            return Err(Stop::refuse("link or special file", &name));
        }
        if !folder
            && !matches!(
                entry.compression(),
                zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
            )
        {
            return Err(Stop::refuse("unsupported compression", &name));
        }
        entries.push(Entry {
            path: segments(&name, folder, false)?,
            kind: if folder {
                Kind::Folder
            } else {
                Kind::File { size: entry.size() }
            },
        });
    }
    let plan = plan(entries, &limits.budget)?;
    create_folders(work, &plan)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(unreadable)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().map_err(unreadable)?.into_owned();
        let path = segments(&name, false, false)?;
        let mut output = Output::create(work, &path, plan.files[&path], shared)?;
        io::copy(&mut entry, &mut output)
            .map_err(|error| recorded(shared, || Stop::unpack(format!("zip: {error}"))))?;
        output.finish()?;
    }
    Ok(())
}

/// The raw names of every central-directory entry, walked by korrid from the
/// end record the `zip` crate also uses: the zip64 end record (APPNOTE
/// 4.3.14) when its locator (4.3.15) sits before the end record (4.3.16).
/// This reader is single-volume, so the disk numbers must be 0 and the
/// on-this-disk count, the total count and the walked count must agree; a
/// forged count could otherwise hide an entry from korrid's own checks.
fn zip_central_names(file: &mut File) -> Result<Vec<Vec<u8>>, Stop> {
    use std::io::{Seek, SeekFrom};
    let corrupt = |why: &str| Stop::unpack(format!("zip: {why}"));
    let le16 = |b: &[u8], at: usize| u64::from(u16::from_le_bytes([b[at], b[at + 1]]));
    let le32 =
        |b: &[u8], at: usize| u64::from(u32::from_le_bytes(b[at..at + 4].try_into().unwrap()));
    let le64 = |b: &[u8], at: usize| u64::from_le_bytes(b[at..at + 8].try_into().unwrap());
    let length = file.metadata()?.len();
    let tail_start = length.saturating_sub(22 + 65_535);
    file.seek(SeekFrom::Start(tail_start))?;
    let mut tail = Vec::new();
    file.take(length - tail_start).read_to_end(&mut tail)?;
    let at = tail
        .windows(4)
        .rposition(|window| window == b"PK\x05\x06")
        .filter(|at| at + 22 <= tail.len())
        .ok_or_else(|| corrupt("no end of central directory"))?;
    let end = &tail[at..at + 22];
    let end_at = tail_start + at as u64;
    let (mut disk, mut cd_disk, mut on_disk, mut total) =
        (le16(end, 4), le16(end, 6), le16(end, 8), le16(end, 10));
    let (mut size, mut offset) = (le32(end, 12), le32(end, 16));
    let mut records_at = end_at;
    if at >= 20 && &tail[at - 20..at - 16] == b"PK\x06\x07" {
        let locator = &tail[at - 20..at];
        if le32(locator, 4) != 0 || le32(locator, 16) != 1 {
            return Err(corrupt("multi-disk zip64 archive"));
        }
        let record_at = le64(locator, 8);
        file.seek(SeekFrom::Start(record_at))?;
        let mut record = [0u8; 56];
        file.read_exact(&mut record)?;
        if &record[..4] != b"PK\x06\x06" {
            return Err(corrupt("no zip64 end of central directory"));
        }
        disk = le32(&record, 16);
        cd_disk = le32(&record, 20);
        on_disk = le64(&record, 24);
        total = le64(&record, 32);
        size = le64(&record, 40);
        offset = le64(&record, 48);
        records_at = record_at;
    }
    if disk != 0 || cd_disk != 0 || on_disk != total {
        return Err(corrupt("end record counts disagree or name another disk"));
    }
    // The central directory ends where the end records begin.
    let start = records_at
        .checked_sub(size)
        .filter(|start| *start == offset)
        .ok_or_else(|| corrupt("central directory is not where the end record says"))?;
    file.seek(SeekFrom::Start(start))?;
    let mut directory = Vec::new();
    file.take(size).read_to_end(&mut directory)?;
    let mut names = Vec::new();
    let mut cursor = 0usize;
    while cursor < directory.len() {
        let header = directory
            .get(cursor..cursor + 46)
            .filter(|header| &header[..4] == b"PK\x01\x02")
            .ok_or_else(|| corrupt("bad central directory entry"))?;
        let (name, extra, comment) = (
            le16(header, 28) as usize,
            le16(header, 30) as usize,
            le16(header, 32) as usize,
        );
        let raw = directory
            .get(cursor + 46..cursor + 46 + name)
            .ok_or_else(|| corrupt("bad central directory entry"))?;
        names.push(raw.to_vec());
        cursor += 46 + name + extra + comment;
    }
    if cursor != directory.len() || names.len() as u64 != total {
        return Err(corrupt(
            "central directory holds a different number of entries",
        ));
    }
    Ok(names)
}

/// The decoder memory one 7z coder asks for.
fn seven_z_memory(coder: &sevenz_rust2::Coder) -> Result<u64, Stop> {
    use sevenz_rust2::EncoderMethod;
    let properties = coder.properties();
    let le32 = |at: usize| {
        properties
            .get(at..at + 4)
            .map(|bytes| u64::from(u32::from_le_bytes(bytes.try_into().unwrap())))
            .ok_or_else(|| Stop::unpack("7z coder properties are too short".into()))
    };
    match coder.encoder_method_id() {
        EncoderMethod::ID_LZMA | EncoderMethod::ID_PPMD => le32(1),
        EncoderMethod::ID_LZMA2 => {
            let bits = u64::from(
                *properties
                    .first()
                    .ok_or_else(|| Stop::unpack("7z coder properties are too short".into()))?,
            );
            if bits > 40 {
                return Err(Stop::unpack("7z dictionary is larger than 4 GiB".into()));
            }
            Ok(if bits == 40 {
                u64::from(u32::MAX)
            } else {
                (2 | (bits & 1)) << (bits / 2 + 11)
            })
        }
        _ => Ok(0),
    }
}

fn seven_z(
    archive: &Path,
    work: &Path,
    limits: &Limits,
    shared: &Rc<RefCell<Shared>>,
) -> Result<(), Stop> {
    use sevenz_rust2::{ArchiveReader, EncoderMethod, Password};
    let unreadable = |error: sevenz_rust2::Error| Stop::unpack(format!("7z: {error}"));
    let mut file = BufReader::new(File::open(archive)?);
    // An encrypted header needs a password and fails here.
    let parsed = sevenz_rust2::Archive::read(&mut file, &Password::empty()).map_err(unreadable)?;
    let mut memory = 0u64;
    for block in &parsed.blocks {
        let mut block_memory = 0u64;
        for coder in &block.coders {
            if coder.encoder_method_id() == EncoderMethod::ID_AES256_SHA256 {
                return Err(Stop::unpack("7z: encrypted content".into()));
            }
            block_memory = block_memory.saturating_add(seven_z_memory(coder)?);
        }
        memory = memory.max(block_memory);
    }
    if memory > limits.memory {
        return Err(Stop::unpack(format!(
            "7z: dictionary of {memory} bytes exceeds MemAvailable of {} bytes",
            limits.memory
        )));
    }
    if parsed.files.len() > limits.budget.max_entries {
        return Err(Stop::unpack(
            "archive exceeds discovery's traversal budget".into(),
        ));
    }
    let mut entries = Vec::with_capacity(parsed.files.len());
    for entry in &parsed.files {
        let folder = entry.is_directory();
        let attributes = entry.windows_attributes();
        // FILE_ATTRIBUTE_REPARSE_POINT is a Windows link; the 0x8000 Unix
        // extension carries st_mode in the high 16 bits.
        if entry.is_anti_item()
            || (entry.has_windows_attributes
                && (attributes & 0x400 != 0
                    || (attributes & 0x8000 != 0 && !unix_type_is_safe(attributes >> 16, folder))))
        {
            return Err(Stop::refuse("link or special file", entry.name()));
        }
        entries.push(Entry {
            path: segments(entry.name(), folder, false)?,
            kind: if folder {
                Kind::Folder
            } else {
                Kind::File { size: entry.size() }
            },
        });
    }
    let plan = plan(entries, &limits.budget)?;
    create_folders(work, &plan)?;
    let mut reader = ArchiveReader::from_archive(parsed, file, Password::empty());
    // One decoder thread, so the dictionary checked above is the memory used.
    reader.set_thread_count(1);
    let mut written = BTreeSet::new();
    let result = reader.for_each_entries(|entry, data| {
        if entry.is_directory() {
            return Ok(true);
        }
        let attempt = (|| {
            let path = segments(entry.name(), false, false)?;
            let mut output = Output::create(work, &path, plan.files[&path], shared)?;
            io::copy(data, &mut output)
                .map_err(|error| recorded(shared, || Stop::unpack(format!("7z: {error}"))))?;
            output.finish()?;
            written.insert(path);
            Ok::<(), Stop>(())
        })();
        match attempt {
            Ok(()) => Ok(true),
            Err(stop) => {
                let reason = stop.reason.clone();
                shared.borrow_mut().failure.get_or_insert(stop);
                Err(sevenz_rust2::Error::Other(reason.into()))
            }
        }
    });
    if let Err(error) = result {
        return Err(recorded(shared, || unreadable(error)));
    }
    if written.len() != plan.files.len() {
        return Err(Stop::unpack("7z: an entry was not decoded".into()));
    }
    Ok(())
}

/// RAR 1.3 to 4.x dictionaries are at most 4 MiB by format.
const LEGACY_RAR_DICTIONARY: u64 = 4 * 1024 * 1024;

fn rar(
    archive: &Path,
    work: &Path,
    limits: &Limits,
    shared: &Rc<RefCell<Shared>>,
) -> Result<(), Stop> {
    use rars::{Archive, ArchiveFamily, ArchiveReadOptions, ArchiveReader, ExtractionDecision};
    let unreadable = |error: rars::Error| Stop::unpack(format!("rar: {error}"));
    let options = ArchiveReadOptions::new().with_rar50_dictionary_size_limit(limits.memory);
    let parsed = ArchiveReader::read_path_with_options(archive, options).map_err(unreadable)?;
    let volume = match &parsed {
        Archive::Rar13(archive) => archive.main.is_volume(),
        Archive::Rar15To40(archive) => archive.main.is_volume(),
        Archive::Rar50Plus(archive) => archive.main.is_volume(),
        _ => return Err(Stop::unpack("rar: unknown archive family".into())),
    };
    if volume {
        return Err(Stop::unpack("rar: multi-volume archive".into()));
    }
    let dictionary = match &parsed {
        Archive::Rar50Plus(archive) => {
            let mut largest = 0;
            for file in archive.files() {
                if !file.is_stored() && !file.is_directory() {
                    largest = largest.max(
                        file.decoded_compression_info()
                            .map_err(unreadable)?
                            .dictionary_size,
                    );
                }
            }
            largest
        }
        _ => LEGACY_RAR_DICTIONARY,
    };
    if dictionary > limits.memory {
        return Err(Stop::unpack(format!(
            "rar: dictionary of {dictionary} bytes exceeds MemAvailable of {} bytes",
            limits.memory
        )));
    }
    let legacy = parsed.family() != ArchiveFamily::Rar50Plus;
    let mut entries = Vec::new();
    for member in parsed.members() {
        if entries.len() >= limits.budget.max_entries {
            return Err(Stop::unpack(
                "archive exceeds discovery's traversal budget".into(),
            ));
        }
        let meta = &member.meta;
        let name = rar_name(meta)?;
        let folder = meta.is_directory;
        if meta.is_encrypted {
            return Err(Stop::refuse("encrypted entry", &name));
        }
        if meta.is_split_before || meta.is_split_after {
            return Err(Stop::refuse("split entry", &name));
        }
        // Host OS numbers: RAR 1.5 to 4.x Unix is 3, RAR 5 Unix is 1;
        // 0x400 is the Windows reparse-point (link) attribute.
        let unix = match parsed.family() {
            ArchiveFamily::Rar15To40 => meta.host_os == Some(3),
            ArchiveFamily::Rar50Plus => meta.host_os == Some(1),
            _ => false,
        };
        let attributes = meta.file_attr;
        if meta.is_redirection
            || (unix && !unix_type_is_safe(attributes as u32, folder))
            || (!unix && attributes & 0x400 != 0)
        {
            return Err(Stop::refuse("link or special file", &name));
        }
        entries.push(Entry {
            path: segments(&name, folder, legacy)?,
            kind: if folder {
                Kind::Folder
            } else {
                Kind::File {
                    size: meta.unpacked_size,
                }
            },
        });
    }
    let plan = plan(entries, &limits.budget)?;
    create_folders(work, &plan)?;
    let mut opened: Vec<Rc<RefCell<Option<Output>>>> = Vec::new();
    let result = parsed.extract_with_control(options, |member| {
        if member.meta.is_directory {
            return Ok(ExtractionDecision::Skip);
        }
        let created = rar_name(&member.meta)
            .and_then(|name| segments(&name, false, legacy))
            .and_then(|path| {
                let size = plan
                    .files
                    .get(&path)
                    .copied()
                    .ok_or_else(|| Stop::unpack("rar: entry was not listed".into()))?;
                Output::create(work, &path, size, shared)
            });
        match created {
            Ok(output) => {
                let slot = Rc::new(RefCell::new(Some(output)));
                opened.push(slot.clone());
                Ok(ExtractionDecision::Extract(Box::new(Slot(slot))))
            }
            Err(stop) => {
                shared.borrow_mut().failure.get_or_insert(stop);
                Err(rars::Error::InvalidArgument("output could not be created"))
            }
        }
    });
    if let Err(error) = result {
        return Err(recorded(shared, || unreadable(error)));
    }
    if opened.len() != plan.files.len() {
        return Err(Stop::unpack("rar: an entry was not decoded".into()));
    }
    for slot in opened {
        let output = slot.borrow_mut().take();
        output
            .ok_or_else(|| Stop::unpack("rar: an entry was not decoded".into()))?
            .finish()?;
    }
    Ok(())
}

/// Names in RAR 5 and Unicode legacy names are UTF-8; other legacy names
/// must be ASCII, because their code page is unknown.
fn rar_name(meta: &rars::ArchiveMemberMeta) -> Result<String, Stop> {
    String::from_utf8(meta.name_bytes().to_vec())
        .map_err(|_| Stop::unpack("rar: entry name is not UTF-8".into()))
}

/// `rars` takes an owned writer; korrid keeps the Output to check its size.
struct Slot(Rc<RefCell<Option<Output>>>);
impl Write for Slot {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0
            .borrow_mut()
            .as_mut()
            .ok_or_else(|| io::Error::other("closed"))?
            .write(buffer)
    }
    fn flush(&mut self) -> io::Result<()> {
        match self.0.borrow_mut().as_mut() {
            Some(output) => output.flush(),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
#[path = "archive_tests.rs"]
mod tests;
