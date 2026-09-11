//! Local synchronous require over a predeclared closed graph. JS roots belong
//! to one evaluator guard, never to a native callback or loader. Weak callbacks
//! cannot keep a context alive after success, exception, or interruption.

use rquickjs::{
    function::This,
    module::{Declarations, Exports, ModuleDef},
    Ctx, Error, Exception, Function, Module, Object, Persistent, Value,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::{Rc, Weak},
};

pub(super) type Imports = Rc<BTreeMap<String, BTreeMap<String, String>>>;

pub(super) const BRIDGE: &str = "korri:closed-require";

pub(super) enum Source {
    CommonJs {
        factory: String,
        edges: BTreeMap<String, String>,
    },
    Json(String),
}

enum LoadedSource {
    CommonJs {
        factory: Persistent<Function<'static>>,
        edges: Rc<BTreeMap<String, String>>,
    },
    Json(String),
}

#[derive(Clone)]
pub(super) struct ModuleRoot<'js>(pub(super) Module<'js>);
// rquickjs 0.9 implements Module's JsLifetime with a bound that its own
// Declared marker lacks. The marker is lifetime-free; only Module's Ctx moves
// between lifetimes. Persistent still checks runtime identity on restoration.
unsafe impl<'js> rquickjs::JsLifetime<'js> for ModuleRoot<'js> {
    type Changed<'to> = ModuleRoot<'to>;
}

#[derive(Default)]
pub(super) struct Registry {
    pub(super) modules: BTreeMap<String, Persistent<ModuleRoot<'static>>>,
    sources: BTreeMap<String, LoadedSource>,
    cache: BTreeMap<String, Persistent<Object<'static>>>,
    imports: Imports,
    evaluating: BTreeMap<String, usize>,
}

impl Registry {
    pub(super) fn new(imports: Imports) -> Self {
        Self {
            imports,
            ..Self::default()
        }
    }
}

/// QuickJS aborts the process when a module that is already evaluating is
/// evaluated again (`js_link_module` asserts the status is not EVALUATING).
/// The engine exposes no module status, and static reachability cannot see a
/// guest callback reached through a global. So the host marks the static import
/// graph it hands to `Module::eval` for the synchronous extent of that call.
/// Outside that extent every module in the graph is evaluated or awaiting, and
/// both states are legal inputs to the engine.
pub(super) struct Evaluating {
    registry: Rc<RefCell<Registry>>,
    names: BTreeSet<String>,
}

impl Evaluating {
    pub(super) fn enter(registry: &Rc<RefCell<Registry>>, root: &str) -> Self {
        let mut state = registry.borrow_mut();
        let imports = Rc::clone(&state.imports);
        let mut names = BTreeSet::new();
        let mut pending = vec![root.to_owned()];
        while let Some(name) = pending.pop() {
            if !names.insert(name.clone()) {
                continue;
            }
            if let Some(edges) = imports.get(&name) {
                pending.extend(edges.values().cloned());
            }
        }
        for name in &names {
            *state.evaluating.entry(name.clone()).or_insert(0) += 1;
        }
        drop(state);
        Self {
            registry: Rc::clone(registry),
            names,
        }
    }

    fn active(registry: &Rc<RefCell<Registry>>, name: &str) -> bool {
        registry.borrow().evaluating.contains_key(name)
    }
}

impl Drop for Evaluating {
    // Nested evaluations share modules, so this counts rather than clears.
    fn drop(&mut self) {
        let mut state = self.registry.borrow_mut();
        for name in &self.names {
            if let Some(count) = state.evaluating.get_mut(name) {
                *count -= 1;
                if *count == 0 {
                    state.evaluating.remove(name);
                }
            }
        }
    }
}

struct Dispatcher {
    registry: Weak<RefCell<Registry>>,
}
// Dispatcher is lifetime-free. Its weak reference contains no borrowed JS
// values; the evaluator guard owns all Persistent roots and drops them first.
unsafe impl<'js> rquickjs::JsLifetime<'js> for Dispatcher {
    type Changed<'to> = Self;
}

pub(super) fn initialize(
    ctx: &Ctx<'_>,
    registry: &Rc<RefCell<Registry>>,
    sources: BTreeMap<String, Source>,
) -> rquickjs::Result<()> {
    ctx.store_userdata(Dispatcher {
        registry: Rc::downgrade(registry),
    })
    .map_err(|_| Error::Unknown)?;
    for (name, source) in sources {
        let source = match source {
            Source::CommonJs { factory, edges } => {
                let factory = ctx.eval::<Function, _>(factory)?;
                LoadedSource::CommonJs {
                    factory: Persistent::save(ctx, factory),
                    edges: Rc::new(edges),
                }
            }
            Source::Json(json) => LoadedSource::Json(json),
        };
        registry.borrow_mut().sources.insert(name, source);
    }
    Ok(())
}

pub(super) struct Bridge;
impl ModuleDef for Bridge {
    fn declare(declarations: &Declarations<'_>) -> rquickjs::Result<()> {
        declarations.declare("load")?;
        Ok(())
    }
    fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
        exports.export("load", Function::new(ctx.clone(), load)?)?;
        Ok(())
    }
}

fn registry(ctx: &Ctx<'_>) -> rquickjs::Result<Rc<RefCell<Registry>>> {
    ctx.userdata::<Dispatcher>()
        .and_then(|d| d.registry.upgrade())
        .ok_or(Error::Unknown)
}

fn load<'js>(ctx: Ctx<'js>, name: String) -> rquickjs::Result<Value<'js>> {
    let registry = registry(&ctx)?;
    // Do not hold a RefCell borrow across guest execution: require is recursive.
    let cached = registry.borrow().cache.get(&name).cloned();
    if let Some(module) = cached {
        return module.restore(&ctx)?.get("exports");
    }
    let module = Object::new(ctx.clone())?;
    // A package can replace Object.prototype. Private cache records must not
    // invoke its setters while the registry is borrowed.
    module.set_prototype(None)?;
    let source = {
        let state = registry.borrow();
        match state.sources.get(&name) {
            Some(LoadedSource::CommonJs { factory, edges }) => {
                Some((factory.clone(), edges.clone()))
            }
            Some(LoadedSource::Json(json)) => {
                let value: Value = ctx.json_parse(json.as_bytes())?;
                module.set("exports", value.clone())?;
                drop(state);
                registry
                    .borrow_mut()
                    .cache
                    .insert(name, Persistent::save(&ctx, module));
                return Ok(value);
            }
            None => None,
        }
    };
    let Some((factory, edges)) = source else {
        let declared = registry
            .borrow()
            .modules
            .get(&name)
            .cloned()
            .ok_or_else(|| Error::new_loading(&name))?;
        if Evaluating::active(&registry, &name) {
            return Err(Exception::throw_type(
                &ctx,
                "require of an ES module that is already evaluating",
            ));
        }
        let evaluating = Evaluating::enter(&registry, &name);
        let evaluated = declared.restore(&ctx)?.0.eval();
        drop(evaluating);
        let (module, promise) = evaluated?;
        // QuickJS also rejects an internal async-function promise when a
        // synchronous ESM initializer throws. Preserve the shared completion
        // rule: even a caught require failure invalidates this invocation. Do
        // not suppress guest rejections by matching their reason to this error.
        // Preparation rejects require edges into top-level-await graphs. Do not
        // run promise jobs from require or reset the shared completion deadline.
        promise.result::<()>().ok_or_else(|| {
            Exception::throw_type(&ctx, "require needs a synchronous ES module")
        })??;
        return Ok(module.namespace()?.into_value());
    };
    let exports = Object::new(ctx.clone())?;
    module.set("exports", exports.clone())?;
    registry
        .borrow_mut()
        .cache
        .insert(name.clone(), Persistent::save(&ctx, module.clone()));
    let require = Function::new(ctx.clone(), move |ctx: Ctx<'js>, request: String| {
        let target = edges.get(&request).ok_or_else(|| {
            Exception::throw_type(&ctx, "require target is not in the closed graph")
        })?;
        load(ctx, target.clone())
    })?;
    let result = factory.restore(&ctx)?.call::<_, Value>((
        This(exports.clone()),
        module.clone(),
        exports,
        require,
    ));
    if let Err(error) = result {
        registry.borrow_mut().cache.remove(&name);
        return Err(error);
    }
    module.get("exports")
}
