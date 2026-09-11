//! Native package metadata over retained bytes only. This is not package
//! discovery: only the snapshot's root node_modules set can supply packages.

use super::source::SourceSnapshot;
use indexmap::IndexMap;
use serde::Deserialize;

// Preserve native condition order without changing serde_json ordering for
// the rest of korrid's wire and persisted data.
#[derive(Deserialize)]
#[serde(untagged)]
enum Value {
    String(String),
    Object(IndexMap<String, Value>),
    Null,
    Unsupported(serde::de::IgnoredAny),
}
impl Value {
    fn as_str(&self) -> Option<&str> {
        if let Self::String(s) = self {
            Some(s)
        } else {
            None
        }
    }
    fn as_object(&self) -> Option<&IndexMap<String, Value>> {
        if let Self::Object(o) = self {
            Some(o)
        } else {
            None
        }
    }
    fn is_object(&self) -> bool {
        self.as_object().is_some()
    }
    fn get(&self, key: &str) -> Option<&Self> {
        self.as_object()?.get(key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Format {
    Esm,
    CommonJs,
    Json,
}

#[derive(Clone, Copy)]
pub(super) enum RequestKind {
    Import,
    Require,
}

pub(super) struct Packages<'a> {
    snapshot: &'a SourceSnapshot,
}

impl<'a> Packages<'a> {
    pub(super) fn new(snapshot: &'a SourceSnapshot) -> Self {
        Self { snapshot }
    }

    fn metadata(&self, directory: &str) -> Result<Option<Value>, String> {
        let name = join(directory, "package.json");
        if self.snapshot.identity(&name).is_err() {
            return Ok(None);
        }
        let value: Value = serde_json::from_slice(self.snapshot.bytes(&name)?)
            .map_err(|error| format!("invalid package metadata {name}: {error}"))?;
        if !value.is_object() {
            return Err(format!("invalid package metadata {name}"));
        }
        Ok(Some(value))
    }

    fn scope(&self, name: &str) -> Result<Option<(String, Value)>, String> {
        let mut directory = parent(name);
        loop {
            if let Some(metadata) = self.metadata(directory)? {
                return Ok(Some((directory.to_owned(), metadata)));
            }
            if directory.is_empty() || directory.rsplit('/').next() == Some("node_modules") {
                return Ok(None);
            }
            directory = parent(directory);
        }
    }

    pub(super) fn format(&self, name: &str) -> Result<Format, String> {
        if name.contains(':') {
            return Err("reserved module identity".into());
        }
        if name.ends_with(".json") {
            return Ok(Format::Json);
        }
        if name.ends_with(".cjs") {
            return Ok(Format::CommonJs);
        }
        if name.ends_with(".mjs") || name.ends_with(".ts") {
            return Ok(Format::Esm);
        }
        if !name.ends_with(".js") {
            return Err("plugin source must be JavaScript or TypeScript".into());
        }
        if let Some((_, metadata)) = self.scope(name)? {
            return match metadata.get("type").and_then(Value::as_str) {
                Some("module") => Ok(Format::Esm),
                None | Some("commonjs") => Ok(Format::CommonJs),
                _ => Err("unsupported package type".into()),
            };
        }
        if name.starts_with("node_modules/") {
            return Err("package metadata is not selected".into());
        }
        // Preserve the pre-existing metadata-free relative JS/TS graph.
        Ok(Format::Esm)
    }

    pub(super) fn resolve(
        &self,
        base: &str,
        request: &str,
        kind: RequestKind,
    ) -> Result<String, String> {
        if request.is_empty()
            || request.contains(['\0', '\\', ':', '?', '#', '%'])
            || request.starts_with('/')
        {
            return Err("unsupported plugin module request".into());
        }
        let name = if request.starts_with("./") || request.starts_with("../") {
            let mut name = relative(parent(base), request)?;
            if matches!(kind, RequestKind::Require) {
                name = self.require_file(&name)?;
            } else if !name.rsplit('/').next().unwrap().contains('.') {
                name.push_str(".ts");
            }
            name
        } else {
            let mut parts: Vec<_> = request.split('/').collect();
            let count = if request.starts_with('@') { 2 } else { 1 };
            // tr46 uses require("punycode/") to request the package directory.
            let directory_request = parts.len() == count + 1 && parts.last() == Some(&"");
            if directory_request {
                parts.pop();
            }
            if parts.len() < count || parts.iter().any(|p| matches!(*p, "" | "." | "..")) {
                return Err("invalid package request".into());
            }
            let root = format!("node_modules/{}", parts[..count].join("/"));
            let metadata = self
                .metadata(&root)?
                .ok_or_else(|| format!("package metadata is not selected: {root}"))?;
            let subpath = if parts.len() == count {
                ".".to_owned()
            } else {
                format!("./{}", parts[count..].join("/"))
            };
            if let Some(exports) = metadata.get("exports") {
                if directory_request {
                    return Err("package directory is not exported".into());
                }
                let target = export_target(exports, &subpath, kind)?;
                if !target.starts_with("./") {
                    return Err("package export target must be relative".into());
                }
                // Exports cannot escape their package, including through wildcard substitutions.
                if target
                    .split('/')
                    .skip(1)
                    .any(|p| matches!(p, "" | "." | ".." | "node_modules"))
                {
                    return Err("invalid package export target".into());
                }
                relative(&root, &target)?
            } else if subpath == "." {
                let main = metadata
                    .get("main")
                    .map(|v| v.as_str().ok_or("invalid package main"))
                    .transpose()?
                    .unwrap_or("index.js");
                let target = if main.starts_with("./") {
                    main.to_owned()
                } else {
                    format!("./{main}")
                };
                let name = relative(&root, &target)?;
                if !name.starts_with(&format!("{root}/")) {
                    return Err("package main escapes its package".into());
                }
                self.require_file(&name)?
            } else {
                self.require_file(&relative(&root, &subpath)?)?
            }
        };
        let name = self.browser_target(&name)?;
        Ok(self
            .snapshot
            .identity(&name)
            .map_err(|e| format!("{e}: {name}"))?
            .to_owned())
    }

    fn require_file(&self, name: &str) -> Result<String, String> {
        if self.snapshot.identity(name).is_ok() {
            return Ok(name.to_owned());
        }
        for suffix in [".js", ".json"] {
            let candidate = format!("{name}{suffix}");
            if self.snapshot.identity(&candidate).is_ok() {
                return Ok(candidate);
            }
        }
        Err(format!("source is not selected: {name}"))
    }

    fn browser_target(&self, name: &str) -> Result<String, String> {
        if let Some((root, metadata)) = self.scope(name)? {
            let local = format!("./{}", name.strip_prefix(&join(&root, "")).unwrap_or(name));
            if let Some(target) = metadata
                .get("browser")
                .and_then(Value::as_object)
                .and_then(|map| map.get(&local))
            {
                let target = target.as_str().ok_or("unsupported browser mapping")?;
                if !target.starts_with("./")
                    || target
                        .split('/')
                        .skip(1)
                        .any(|p| matches!(p, "" | "." | ".." | "node_modules"))
                {
                    return Err("invalid browser target".into());
                }
                return relative(&root, target);
            }
        }
        Ok(name.to_owned())
    }
}

fn export_target(exports: &Value, subpath: &str, kind: RequestKind) -> Result<String, String> {
    let mut capture = None;
    let target = if let Some(map) = exports
        .as_object()
        .filter(|map| map.keys().any(|key| key.starts_with('.')))
    {
        if map.keys().any(|key| !key.starts_with('.')) {
            return Err("mixed package export keys".into());
        }
        if let Some(exact) = map.get(subpath) {
            exact
        } else {
            let mut best = None;
            for (key, value) in map {
                if let Some((prefix, suffix)) = key.split_once('*') {
                    if subpath.len() >= prefix.len() + suffix.len()
                        && subpath.starts_with(prefix)
                        && subpath.ends_with(suffix)
                    {
                        let rank = (prefix.len(), key.len());
                        if best.as_ref().is_none_or(|(old, _, _)| rank > *old) {
                            best = Some((
                                rank,
                                value,
                                &subpath[prefix.len()..subpath.len() - suffix.len()],
                            ));
                        }
                    }
                }
            }
            let (_, value, matched) = best.ok_or("package subpath is not exported")?;
            capture = Some(matched);
            value
        }
    } else if subpath == "." {
        exports
    } else {
        return Err("package subpath is not exported".into());
    };
    let target = condition(target, kind)?.ok_or("package subpath is not exported")?;
    Ok(match capture {
        Some(capture) => target.replace('*', capture),
        None => target.to_owned(),
    })
}

fn condition(value: &Value, kind: RequestKind) -> Result<Option<&str>, String> {
    match value {
        Value::String(target) => Ok(Some(target)),
        Value::Null => Err("package subpath is not exported".into()),
        Value::Object(map) => {
            // Native metadata order is required: default is not a fallback
            // pass after browser/import/require; the first active key wins.
            for (key, value) in map {
                if key == "default"
                    || key == "browser"
                    || key
                        == match kind {
                            RequestKind::Import => "import",
                            RequestKind::Require => "require",
                        }
                {
                    if let Some(target) = condition(value, kind)? {
                        return Ok(Some(target));
                    }
                }
            }
            Ok(None)
        }
        _ => Err("unsupported package export target".into()),
    }
}

fn parent(name: &str) -> &str {
    name.rsplit_once('/').map_or("", |(parent, _)| parent)
}
fn join(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}/{name}")
    }
}
fn relative(base: &str, request: &str) -> Result<String, String> {
    if request.contains(['\0', '\\', ':', '?', '#', '%'])
        || matches!(request.rsplit('/').next(), Some("" | "." | ".."))
    {
        return Err("plugin import must name a source file".into());
    }
    let mut parts: Vec<_> = base.split('/').filter(|p| !p.is_empty()).collect();
    for part in request.split('/') {
        match part {
            "." => {}
            ".." => {
                parts.pop().ok_or("plugin import escapes its snapshot")?;
            }
            "" => return Err("plugin import must name a source file".into()),
            part => parts.push(part),
        }
    }
    Ok(parts.join("/"))
}
