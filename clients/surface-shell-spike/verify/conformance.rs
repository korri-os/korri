use crate::{
    index,
    quickjs::Runner,
    view::{kebab, Converter},
};
use anyhow::{bail, Context, Result};
use i_slint_compiler::langtype::Type;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path, time::Duration};

pub const VERSION: &str = "throwaway-1";
pub const SUMMARY: &str = ".spike-verification.json";
pub const ACTIONS: [&str; 8] = [
    "up", "down", "left", "right", "confirm", "back", "menu", "options",
];
pub const MEMORY: usize = 64 * 1024 * 1024;
pub const BUDGET_MS: u64 = 250;

fn describe(ty: &Type) -> Value {
    match ty {
        Type::Array(item) => json!({"array":describe(item)}),
        Type::Struct(item) => {
            json!({"struct":item.fields.iter().map(|(k,v)| (k.to_string(), describe(v))).collect::<BTreeMap<_,_>>()})
        }
        other => json!(other.to_string()),
    }
}

fn check_value(
    value: &Value,
    ty: &Type,
    path: &str,
    package: &Path,
    images: &mut Converter,
) -> Result<()> {
    // CONTRACT.md: null and omitted fields retain their previous value.
    if value.is_null() {
        return Ok(());
    }
    let valid = match (value, ty) {
        (Value::Bool(_), Type::Bool) | (Value::String(_), Type::String) => true,
        (Value::Number(n), Type::Int32) => n
            .as_f64()
            .is_some_and(|n| n >= i32::MIN as f64 && n <= i32::MAX as f64),
        (Value::Number(n), Type::Float32) => n.as_f64().is_some_and(|n| (n as f32).is_finite()),
        (Value::Array(items), Type::Array(item)) => {
            for (i, v) in items.iter().enumerate() {
                check_value(v, item, &format!("{path}[{i}]"), package, images)?;
            }
            true
        }
        (Value::Object(fields), Type::Struct(s)) if !fields.contains_key("$image") => {
            for (name, v) in fields {
                let field = kebab(name);
                let ty = s
                    .fields
                    .get(field.as_str())
                    .with_context(|| format!("{path}: undeclared field {field}"))?;
                check_value(v, ty, &format!("{path}.{field}"), package, images)?;
            }
            true
        }
        (Value::Object(fields), Type::Image)
            if fields.len() == 1 && fields.contains_key("$image") =>
        {
            images
                .convert(value, &slint_interpreter::Value::Void, package)
                .with_context(|| format!("{path}: image decode"))?;
            true
        }
        _ => false,
    };
    if !valid {
        bail!("{path}: wrong type, expected {ty}, got {value}");
    }
    Ok(())
}

fn check_view(
    value: &Value,
    types: &BTreeMap<String, Type>,
    package: &Path,
    images: &mut Converter,
) -> Result<()> {
    for (name, value) in value.as_object().context("view must return an object")? {
        let name = kebab(name);
        let ty = types
            .get(&name)
            .with_context(|| format!("undeclared property {name}"))?;
        check_value(value, ty, &name, package, images)?;
    }
    Ok(())
}

pub fn verify(package: &Path, model: &str, write: bool) -> Result<Value> {
    let digest = index::digest(package, false)?;
    let compiler = slint_interpreter::Compiler::default();
    let compilation = pollster::block_on(compiler.build_from_path(package.join("main.slint")));
    let diagnostics: Vec<_> = compilation.diagnostics().map(|d| d.to_string()).collect();
    let mut summary = json!({"spike_only":true,"verifier_version":VERSION,"package_sha256":digest,"fixture_sha256":format!("{:x}", Sha256::digest(model.as_bytes())),"slint_version":"1.18.1", "diagnostics": diagnostics,"properties":{},"sweep":{"seed":12648430,"random_actions":500,"completed_views":0},"memory_limit_bytes":MEMORY,"call_budget_ms":BUDGET_MS,"max_call_ms":0,"peak_quickjs_backing_bytes":0,"pass":false,"reasons":[]});
    let mut runner = Runner::new(MEMORY, Duration::from_millis(BUDGET_MS))?;
    let mut completed = 0;
    let outcome = (|| -> Result<()> {
        if compilation.has_errors() {
            bail!("Slint compilation failed");
        }
        let definition = compilation
            .component("Surface")
            .context("main.slint must export Surface")?;
        let public: BTreeMap<_, _> = definition.properties().collect();
        let types: BTreeMap<_, _> = definition
            .properties_and_callbacks()
            .filter(|(n, _)| public.contains_key(n))
            .map(|(n, (t, _))| (n, t))
            .collect();
        summary["properties"] = json!(types
            .iter()
            .map(|(n, t)| (n.clone(), describe(t)))
            .collect::<BTreeMap<_, _>>());
        let source = std::fs::read_to_string(package.join("logic.js"))?;
        runner.load(&source, model)?;
        runner.init()?;
        let mut images = Converter::default();
        let mut check = |runner: &mut Runner| -> Result<()> {
            let (value, _, _) = runner.view()?;
            check_view(&value, &types, package, &mut images)?;
            completed += 1;
            Ok(())
        };
        check(&mut runner)?;
        for action in ACTIONS {
            runner.update(action)?;
            check(&mut runner)?;
        }
        let mut seed = 0x00c0_ffee_u32;
        for _ in 0..500 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            runner.update(ACTIONS[seed as usize % ACTIONS.len()])?;
            check(&mut runner)?;
        }
        Ok(())
    })();
    summary["sweep"]["completed_views"] = json!(completed);
    summary["max_call_ms"] = json!(runner.max_call_ms);
    summary["peak_quickjs_backing_bytes"] = json!(runner.peak_bytes());
    match outcome {
        Ok(()) => summary["pass"] = json!(true),
        Err(e) => summary["reasons"] = json!([format!("{e:#}")]),
    }
    if write {
        std::fs::write(package.join(SUMMARY), serde_json::to_vec_pretty(&summary)?)?;
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn planted(body: &str) -> Result<Value> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("main.slint"), "export struct Row { label: string } export component Surface inherits Window { in property <int> count; in property <[Row]> rows; in property <image> art; }")?;
        std::fs::write(
            dir.path().join("logic.js"),
            format!("globalThis.surface = {{init:()=>0, update:s=>s, view:()=>{{ {body} }} }}"),
        )?;
        verify(dir.path(), "{}", false)
    }
    fn rejects(body: &str, reason: &str) -> Result<()> {
        let result = planted(body)?;
        assert_eq!(result["pass"], false, "{result}");
        assert!(result["reasons"].to_string().contains(reason), "{result}");
        Ok(())
    }
    #[test]
    fn unknown_property() -> Result<()> {
        rejects("return {notDeclared: null}", "undeclared property")
    }
    #[test]
    fn wrong_type() -> Result<()> {
        rejects("return {count:'no'}", "wrong type")
    }
    #[test]
    fn recursive_type() -> Result<()> {
        rejects("return {rows:[{label:false}]}", "rows[0].label")
    }
    #[test]
    fn throws() -> Result<()> {
        rejects("throw Error('planted throw')", "QuickJS")
    }
    #[test]
    fn infinite_loop() -> Result<()> {
        rejects("while(true) {}", "budget")
    }
    #[test]
    fn memory_blow_up() -> Result<()> {
        rejects(
            "let a=[]; while(true) { a.push(new Uint8Array(1024*1024)); }",
            "QuickJS",
        )
    }
    #[test]
    fn image_decode() -> Result<()> {
        rejects(
            "return {art:{$image:'data:image/png;base64,AAAA'}}",
            "image decode",
        )
    }
    #[test]
    fn accepts_null_and_partial_structs() -> Result<()> {
        assert_eq!(
            planted("return {count: null, rows:[{label:'ok'},{}]}")?["pass"],
            true
        );
        Ok(())
    }
}
