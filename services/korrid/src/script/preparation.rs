//! In-process preparation of a closed source graph. Admission and generated
//! output have byte ceilings; Oxc has no hard time or host-memory limit. A
//! pathological parse can stall or crash korrid. No VM deadline covers this work.

use std::{cell::Cell, collections::BTreeMap, path::Path, rc::Rc};

use oxc::{
    allocator::Allocator,
    codegen::Codegen,
    parser::Parser,
    semantic::SemanticBuilder,
    span::SourceType,
    transformer::{TransformOptions, Transformer},
};
use rquickjs::{
    loader::{Loader, Resolver},
    Ctx, Error, Module, Runtime,
};

use super::source::{SourceSnapshot, PLUGIN_SOURCE_BYTES};

pub(super) const JAVASCRIPT_BYTES: usize = 512 * 1024;

pub(super) struct PreparedGraph {
    entry: String,
    modules: BTreeMap<String, String>,
    edges: BTreeMap<String, BTreeMap<String, String>>,
}

impl PreparedGraph {
    pub(super) fn new(snapshot: &SourceSnapshot, entry: &str) -> Result<Self, String> {
        let entry = snapshot.identity(entry)?.to_owned();
        let mut graph = Self {
            entry: entry.clone(),
            modules: BTreeMap::new(),
            edges: BTreeMap::new(),
        };
        let mut pending = vec![entry];
        let mut remaining = JAVASCRIPT_BYTES;
        while let Some(name) = pending.pop() {
            if graph.modules.contains_key(&name) {
                continue;
            }
            let javascript = transpile(snapshot.text(&name)?, &name)?;
            remaining = remaining
                .checked_sub(javascript.len())
                .ok_or("plugin JavaScript exceeds 512 KiB")?;
            // Read the emitted module record, not TS import heuristics. Oxc
            // erases `import type`, but keeps unused value imports and the
            // side effect of `import { type T }` (emitted as `import {}`).
            let allocator = Allocator::default();
            let parsed = Parser::new(&allocator, &javascript, SourceType::mjs()).parse();
            if let Some(first) = parsed.diagnostics.first() {
                return Err(format!("plugin JavaScript failed to parse: {first}"));
            }
            let mut edges = BTreeMap::new();
            for request in parsed.module_record.requested_modules.keys() {
                let identity = resolve(snapshot, &name, request.as_str())?;
                pending.push(identity.clone());
                edges.insert(request.to_string(), identity);
            }
            graph.edges.insert(name.clone(), edges);
            graph.modules.insert(name, javascript);
        }
        Ok(graph)
    }

    pub(super) fn install(mut self, runtime: &Runtime) -> (String, String, Rc<Cell<bool>>) {
        let linking = Rc::new(Cell::new(true));
        let source = self.modules.remove(&self.entry).expect("prepared entry");
        runtime.set_loader(
            ClosedResolver {
                edges: self.edges,
                linking: Rc::clone(&linking),
            },
            ClosedLoader(self.modules),
        );
        (self.entry, source, linking)
    }
}

pub(super) fn transpile(source: &str, name: &str) -> Result<String, String> {
    let source_type = if name.ends_with(".ts") {
        if source.len() > PLUGIN_SOURCE_BYTES {
            return Err("plugin source exceeds 128 KiB".into());
        }
        SourceType::ts()
    } else if name.ends_with(".js") {
        if source.len() > JAVASCRIPT_BYTES {
            return Err("plugin JavaScript exceeds 512 KiB".into());
        }
        SourceType::mjs()
    } else {
        return Err("plugin source must be JavaScript or TypeScript".into());
    };
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if let Some(first) = parsed.diagnostics.first() {
        return Err(format!("plugin failed to parse: {first}"));
    }
    if !parsed.module_record.dynamic_imports.is_empty() {
        return Err("plugin dynamic imports are not permitted".into());
    }
    if !parsed.module_record.import_metas.is_empty() {
        return Err("plugin import.meta is not permitted".into());
    }
    let mut program = parsed.program;
    // Required by Oxc's TS enum lowering; otherwise enum transformation panics.
    let semantic = SemanticBuilder::new().with_enum_eval(true).build(&program);
    if let Some(first) = semantic.diagnostics.first() {
        return Err(format!("plugin failed semantic analysis: {first}"));
    }
    let scoping = semantic.semantic.into_scoping();
    let mut options = TransformOptions::default();
    options.typescript.only_remove_type_imports = true;
    let transformed = Transformer::new(&allocator, Path::new(name), &options)
        .build_with_scoping(scoping, &mut program);
    if let Some(first) = transformed.diagnostics.first() {
        return Err(format!("plugin failed to transpile: {first}"));
    }
    Ok(Codegen::new().build(&program).code)
}

fn resolve(snapshot: &SourceSnapshot, base: &str, request: &str) -> Result<String, String> {
    if !(request.starts_with("./") || request.starts_with("../"))
        || request.contains(['\0', '\\', ':', '?', '#', '%'])
    {
        return Err("plugin imports must be relative source paths".into());
    }
    if matches!(request.rsplit('/').next(), Some("" | "." | "..")) {
        return Err("plugin import must name a source file".into());
    }
    let mut parts: Vec<_> = base.split('/').collect();
    parts.pop();
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
    let filename = parts
        .last()
        .ok_or("plugin import must name a source file")?;
    let mut name = parts.join("/");
    // Current helpers use extensionless TS imports (retroarch/render-settings.ts).
    // Explicit .js/.ts requests are exact: no package/index or .js-to-.ts fallback.
    if !filename.contains('.') {
        name.push_str(".ts");
    } else if !filename.ends_with(".ts") && !filename.ends_with(".js") {
        return Err("plugin import must name JavaScript or TypeScript".into());
    }
    Ok(snapshot.identity(&name)?.to_owned())
}

struct ClosedResolver {
    edges: BTreeMap<String, BTreeMap<String, String>>,
    linking: Rc<Cell<bool>>,
}

impl Resolver for ClosedResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
    ) -> rquickjs::Result<String> {
        if self.linking.get() {
            if let Some(identity) = self.edges.get(base).and_then(|edges| edges.get(name)) {
                return Ok(identity.clone());
            }
        }
        Err(Error::new_resolving(base, name))
    }
}

struct ClosedLoader(BTreeMap<String, String>);

impl Loader for ClosedLoader {
    fn load<'js>(&mut self, ctx: &Ctx<'js>, name: &str) -> rquickjs::Result<Module<'js>> {
        let source = self.0.get(name).ok_or_else(|| Error::new_loading(name))?;
        Module::declare(ctx.clone(), name, source.as_bytes())
    }
}
