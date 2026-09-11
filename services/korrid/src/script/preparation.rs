//! In-process preparation of a closed source graph. Admission and generated
//! output have byte ceilings; Oxc has no hard time or host-memory limit. A
//! pathological parse can stall or crash korrid. No VM deadline covers this work.

use super::{
    commonjs::{self, Registry},
    packages::{Format, Packages, RequestKind},
    source::{SourceSnapshot, PLUGIN_SOURCE_BYTES},
};
use oxc::{
    allocator::Allocator,
    ast::{
        ast::{Argument, Expression},
        AstKind,
    },
    codegen::Codegen,
    parser::Parser,
    semantic::SemanticBuilder,
    span::SourceType,
    transformer::{TransformOptions, Transformer},
};
use rquickjs::{
    loader::{Loader, Resolver},
    module::Evaluated,
    Ctx, Error, Module, Persistent, Promise, Runtime,
};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    path::Path,
    rc::{Rc, Weak},
};

pub(super) const JAVASCRIPT_BYTES: usize = 512 * 1024;
// Measured original pinned graphs: Schema 2,698,372 bytes / 102 modules;
// whatwg-url 625,525 bytes / 49 modules. Keep the single-file ceiling separate.
const GRAPH_BYTES: usize = 4 * 1024 * 1024;

pub(super) struct PreparedGraph {
    entry: String,
    modules: BTreeMap<String, String>,
    edges: BTreeMap<String, BTreeMap<String, String>>,
    sources: BTreeMap<String, commonjs::Source>,
}

impl PreparedGraph {
    pub(super) fn new(snapshot: &SourceSnapshot, entry: &str) -> Result<Self, String> {
        let entry = snapshot.identity(entry)?.to_owned();
        let packages = Packages::new(snapshot);
        if packages.format(&entry)? != Format::Esm {
            return Err("plugin entry must be an ES module".into());
        }
        let mut graph = Self {
            entry: entry.clone(),
            modules: BTreeMap::new(),
            edges: BTreeMap::new(),
            sources: BTreeMap::new(),
        };
        let mut pending = vec![entry];
        let mut bytes = 0usize;
        let mut async_modules = BTreeSet::new();
        let mut requires = Vec::new();
        while let Some(name) = pending.pop() {
            if graph.modules.contains_key(&name) {
                continue;
            }
            let format = packages.format(&name)?;
            let source = snapshot.text(&name)?;
            let mut edges = BTreeMap::new();
            let javascript = match format {
                Format::Esm => {
                    let javascript = transpile(source, &name)?;
                    // The graph allowance does not enlarge any emitted module.
                    // Check after Oxc, before dependency analysis or VM creation.
                    check_javascript_size(&javascript)?;
                    // Read the emitted module record, not TS import heuristics.
                    // True type-only imports disappear; unused value imports stay.
                    let analysis = analyze(&javascript, SourceType::mjs())?;
                    if analysis.top_level_await {
                        async_modules.insert(name.clone());
                    }
                    for request in analysis.imports {
                        let identity = packages.resolve(&name, &request, RequestKind::Import)?;
                        if packages.format(&identity)? == Format::Json {
                            return Err("ES module JSON imports are not supported".into());
                        }
                        pending.push(identity.clone());
                        edges.insert(request, identity);
                    }
                    javascript
                }
                Format::CommonJs => {
                    check_javascript_size(source)?;
                    let analysis = analyze(source, SourceType::cjs())?;
                    let mut local = BTreeMap::new();
                    for request in analysis.requires {
                        let identity = packages.resolve(&name, &request, RequestKind::Require)?;
                        requires.push((name.clone(), identity.clone()));
                        pending.push(identity.clone());
                        local.insert(request, identity);
                    }
                    // Keep the original program inside its own function scope.
                    // No renaming, global require, or evaluation-time transpilation.
                    let factory = format!("(function(module, exports, require) {{\n{source}\n}})");
                    check_javascript_size(&factory)?;
                    bytes = bytes
                        .checked_add(factory.len())
                        .ok_or("plugin graph size overflow")?;
                    graph.sources.insert(
                        name.clone(),
                        commonjs::Source::CommonJs {
                            factory,
                            edges: local,
                        },
                    );
                    edges.insert(commonjs::BRIDGE.to_owned(), commonjs::BRIDGE.to_owned());
                    facade(&name)
                }
                Format::Json => {
                    check_javascript_size(source)?;
                    serde_json::from_str::<serde_json::Value>(source)
                        .map_err(|e| format!("invalid module JSON {name}: {e}"))?;
                    bytes = bytes
                        .checked_add(source.len())
                        .ok_or("plugin graph size overflow")?;
                    graph
                        .sources
                        .insert(name.clone(), commonjs::Source::Json(source.to_owned()));
                    edges.insert(commonjs::BRIDGE.to_owned(), commonjs::BRIDGE.to_owned());
                    facade(&name)
                }
            };
            check_javascript_size(&javascript)?;
            bytes = bytes
                .checked_add(javascript.len())
                .ok_or("plugin graph size overflow")?;
            graph.edges.insert(name.clone(), edges);
            graph.modules.insert(name, javascript);
        }
        for (base, target) in requires {
            let esm = packages.format(&target)? == Format::Esm;
            let mut pending = vec![target];
            let mut visited = BTreeSet::new();
            while let Some(name) = pending.pop() {
                if !visited.insert(name.clone()) {
                    continue;
                }
                if esm && name == base {
                    // QuickJS cannot re-enter Module::eval for an ESM already
                    // evaluating. Reject mixed cycles before touching the VM.
                    return Err("cyclic require of an ES module is not supported".into());
                }
                if async_modules.contains(&name) {
                    return Err("require cannot load a top-level-await ES module graph".into());
                }
                if let Some(edges) = graph.edges.get(&name) {
                    pending.extend(edges.values().cloned());
                }
                if let Some(commonjs::Source::CommonJs { edges, .. }) = graph.sources.get(&name) {
                    pending.extend(edges.values().cloned());
                }
            }
        }
        if bytes > GRAPH_BYTES {
            return Err(format!("plugin JavaScript graph exceeds 4 MiB: prepared graph has {bytes} bytes across {} modules", graph.modules.len()));
        }
        Ok(graph)
    }

    pub(super) fn install(self, runtime: &Runtime) -> InstalledGraph {
        let linking = Rc::new(Cell::new(true));
        // The resolver and the evaluation-state guard read one immutable import
        // graph. Nothing can extend it after preparation.
        let edges: commonjs::Imports = Rc::new(self.edges);
        let registry = Rc::new(RefCell::new(Registry::new(Rc::clone(&edges))));
        let modules = Rc::new(self.modules);
        runtime.set_loader(
            ClosedResolver {
                edges,
                linking: Rc::clone(&linking),
            },
            ClosedLoader {
                modules: Rc::clone(&modules),
                registry: Rc::downgrade(&registry),
            },
        );
        InstalledGraph {
            entry: self.entry,
            modules,
            sources: self.sources,
            registry,
            linking,
        }
    }
}

fn facade(name: &str) -> String {
    format!(
        "import {{ load }} from '{}'; export default load({});",
        commonjs::BRIDGE,
        serde_json::to_string(name).expect("string JSON")
    )
}

pub(super) struct InstalledGraph {
    entry: String,
    modules: Rc<BTreeMap<String, String>>,
    sources: BTreeMap<String, commonjs::Source>,
    registry: Rc<RefCell<Registry>>,
    linking: Rc<Cell<bool>>,
}

impl InstalledGraph {
    /// Declare the closed graph, then evaluate its entry under the shared
    /// evaluation-state guard. Native re-entry from a guest callback fails as a
    /// bounded plugin error instead of aborting the host.
    pub(super) fn evaluate<'js>(
        &mut self,
        ctx: &Ctx<'js>,
    ) -> Result<(Module<'js, Evaluated>, Promise<'js>), String> {
        let entry = self.declare(ctx)?;
        let evaluating = commonjs::Evaluating::enter(&self.registry, &self.entry);
        let evaluated = entry.eval();
        drop(evaluating);
        evaluated.map_err(|error| format!("plugin evaluation failed: {error}"))
    }

    fn declare<'js>(&mut self, ctx: &Ctx<'js>) -> Result<Module<'js>, String> {
        commonjs::initialize(ctx, &self.registry, std::mem::take(&mut self.sources))
            .map_err(|error| format!("plugin CommonJS initialization failed: {error}"))?;
        // Even lazy require targets are declared before execution. Nothing can
        // add source, parse code, or reopen resolution while a plugin runs.
        let mut loader = ClosedLoader {
            modules: Rc::clone(&self.modules),
            registry: Rc::downgrade(&self.registry),
        };
        for name in self.modules.keys() {
            if !self.registry.borrow().modules.contains_key(name) {
                loader
                    .load(ctx, name)
                    .map_err(|error| format!("plugin evaluation failed: {error}"))?;
            }
        }
        self.linking.set(false);
        self.registry
            .borrow()
            .modules
            .get(&self.entry)
            .expect("prepared entry")
            .clone()
            .restore(ctx)
            .map(|root| root.0)
            .map_err(|error| error.to_string())
    }
}

pub(super) fn transpile(source: &str, name: &str) -> Result<String, String> {
    let source_type = if name.ends_with(".ts") {
        if source.len() > PLUGIN_SOURCE_BYTES {
            return Err("plugin source exceeds 128 KiB".into());
        }
        SourceType::ts()
    } else if name.ends_with(".js") || name.ends_with(".mjs") {
        check_javascript_size(source)?;
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

fn check_javascript_size(source: &str) -> Result<(), String> {
    if source.len() > JAVASCRIPT_BYTES {
        Err("plugin JavaScript exceeds 512 KiB".into())
    } else {
        Ok(())
    }
}

struct Analysis {
    imports: Vec<String>,
    requires: Vec<String>,
    top_level_await: bool,
}
fn analyze(source: &str, source_type: SourceType) -> Result<Analysis, String> {
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
    let mut result = Analysis {
        imports: parsed
            .module_record
            .requested_modules
            .keys()
            .map(ToString::to_string)
            .collect(),
        requires: Vec::new(),
        top_level_await: false,
    };
    let semantic = SemanticBuilder::new()
        .with_build_nodes(true)
        .with_check_syntax_error(true)
        .build(&parsed.program);
    if let Some(first) = semantic.diagnostics.first() {
        return Err(format!("plugin failed semantic analysis: {first}"));
    }
    let semantic = semantic.semantic;
    for node in semantic.nodes().iter() {
        if (matches!(node.kind(), AstKind::AwaitExpression(_))
            || matches!(node.kind(), AstKind::ForOfStatement(statement) if statement.r#await))
            && !semantic
                .scoping()
                .scope_ancestors(node.scope_id())
                .any(|scope| semantic.scoping().scope_flags(scope).is_function())
        {
            result.top_level_await = true;
        }
        if source_type.is_commonjs() {
            if let AstKind::CallExpression(call) = node.kind() {
                if let Expression::Identifier(identifier) = &call.callee {
                    if identifier.name == "require"
                        && identifier.reference_id.get().is_some_and(|id| {
                            semantic.scoping().get_reference(id).symbol_id().is_none()
                        })
                    {
                        match call.arguments.as_slice() {
                            [Argument::StringLiteral(request)] if !call.optional => {
                                result.requires.push(request.value.to_string())
                            }
                            _ => {
                                return Err(
                                    "CommonJS require must have one literal source request".into()
                                )
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(result)
}

struct ClosedResolver {
    edges: commonjs::Imports,
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

struct ClosedLoader {
    modules: Rc<BTreeMap<String, String>>,
    registry: Weak<RefCell<Registry>>,
}
impl Loader for ClosedLoader {
    fn load<'js>(&mut self, ctx: &Ctx<'js>, name: &str) -> rquickjs::Result<Module<'js>> {
        if name == commonjs::BRIDGE {
            return Module::declare_def::<commonjs::Bridge, _>(ctx.clone(), name);
        }
        let registry = self.registry.upgrade().ok_or(Error::Unknown)?;
        let existing = registry.borrow().modules.get(name).cloned();
        if let Some(module) = existing {
            return module.restore(ctx).map(|root| root.0);
        }
        let source = self
            .modules
            .get(name)
            .ok_or_else(|| Error::new_loading(name))?;
        let module = Module::declare(ctx.clone(), name, source.as_bytes())?;
        registry.borrow_mut().modules.insert(
            name.to_owned(),
            Persistent::save(ctx, commonjs::ModuleRoot(module.clone())),
        );
        Ok(module)
    }
}
