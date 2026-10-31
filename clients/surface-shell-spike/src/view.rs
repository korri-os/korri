//! THROWAWAY: the CONTRACT.md JSON boundary, with one decoded-image cache.
use anyhow::{bail, Context, Result};
use base64::Engine;
use i_slint_compiler::langtype::Type;
use serde_json::Value as Json;
use slint::{Image, Model, ModelRc, VecModel};
#[cfg(test)]
use slint_interpreter::ValueType;
use slint_interpreter::{ComponentInstance, Struct, Value};
use std::{collections::HashMap, path::Path};

#[derive(Default)]
pub struct Converter {
    images: HashMap<String, Image>,
}

pub fn kebab(name: &str) -> String {
    let mut result = String::new();
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            result.push('-');
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

// Public properties() exposes Number for both int and float. Use the pinned
// interpreter's detailed metadata only for recursive numeric coercion.
fn coerce_numbers(value: &mut Value, ty: &Type) -> Result<()> {
    match (value, ty) {
        (Value::Number(number), Type::Int32) => {
            let integer = number.trunc();
            if integer < i32::MIN as f64 || integer > i32::MAX as f64 {
                bail!("integer outside Slint int range");
            }
            *number = integer;
        }
        (Value::Number(number), Type::Float32) => {
            let float = *number as f32;
            if !float.is_finite() {
                bail!("number outside Slint float range");
            }
            *number = float as f64;
        }
        (Value::Struct(value), Type::Struct(ty)) => {
            for (name, ty) in &ty.fields {
                if let Some(mut field) = value.get_field(name).cloned() {
                    coerce_numbers(&mut field, ty)?;
                    value.set_field(name.to_string(), field);
                }
            }
        }
        (Value::Model(value), Type::Array(ty)) => {
            let mut items = Vec::new();
            for i in 0..value.row_count() {
                let mut item = value.row_data(i).context("model row missing")?;
                coerce_numbers(&mut item, ty)?;
                items.push(item);
            }
            *value = ModelRc::new(VecModel::from(items));
        }
        _ => {}
    }
    Ok(())
}

impl Converter {
    fn image(&mut self, source: &str, package: &Path) -> Result<Image> {
        let key = if source.starts_with("data:") {
            source.to_owned()
        } else {
            package.join(source).to_string_lossy().into_owned()
        };
        if let Some(image) = self.images.get(&key) {
            return Ok(image.clone());
        }
        let bytes = if source.starts_with("data:") {
            let (header, data) = source.split_once(',').context("invalid image data URL")?;
            if header.ends_with(";base64") {
                base64::engine::general_purpose::STANDARD.decode(data)?
            } else {
                let mut bytes = Vec::new();
                let mut chars = data.bytes();
                while let Some(c) = chars.next() {
                    if c == b'%' {
                        let digits = [
                            chars.next().context("short URL escape")?,
                            chars.next().context("short URL escape")?,
                        ];
                        bytes.push(u8::from_str_radix(std::str::from_utf8(&digits)?, 16)?);
                    } else {
                        bytes.push(c);
                    }
                }
                bytes
            }
        } else {
            std::fs::read(&key).with_context(|| format!("image {key}"))?
        };
        let format = if source.starts_with("data:image/svg+xml")
            || Path::new(source)
                .extension()
                .is_some_and(|ext| ext == "svg")
        {
            Some("svg")
        } else {
            None
        };
        let image = Image::load_from_data(&bytes, format).context("decode image")?;
        self.images.insert(key, image.clone());
        Ok(image)
    }

    pub fn convert(&mut self, json: &Json, previous: &Value, package: &Path) -> Result<Value> {
        Ok(match json {
            Json::Null => previous.clone(),
            Json::Bool(v) => Value::Bool(*v),
            Json::Number(v) => Value::Number(v.as_f64().context("number outside f64 range")?),
            Json::String(v) => Value::String(v.as_str().into()),
            Json::Array(items) => {
                let old = match previous {
                    Value::Model(m) => Some(m),
                    _ => None,
                };
                let values = items
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let prior = old.and_then(|m| m.row_data(i)).unwrap_or_default();
                        self.convert(v, &prior, package)
                    })
                    .collect::<Result<Vec<_>>>()?;
                Value::Model(ModelRc::new(VecModel::from(values)))
            }
            Json::Object(fields) => {
                if let Some(image) = fields.get("$image") {
                    Value::Image(
                        self.image(image.as_str().context("$image must be a string")?, package)?,
                    )
                } else {
                    let mut value = match previous {
                        Value::Struct(s) => s.clone(),
                        _ => Struct::default(),
                    };
                    for (name, json) in fields {
                        let name = kebab(name);
                        let prior = value.get_field(&name).cloned().unwrap_or_default();
                        value.set_field(name, self.convert(json, &prior, package)?);
                    }
                    Value::Struct(value)
                }
            }
        })
    }

    pub fn apply(
        &mut self,
        component: &ComponentInstance,
        view: &Json,
        package: &Path,
    ) -> Result<()> {
        let fields = view.as_object().context("view() must return an object")?;
        let definition = component.definition();
        let types: HashMap<_, _> = definition.properties().collect();
        let detailed: HashMap<_, _> = definition
            .properties_and_callbacks()
            .map(|(name, (ty, _))| (name, ty))
            .collect();
        for (name, json) in fields {
            if json.is_null() {
                continue;
            }
            let property = kebab(name);
            let declared = types
                .get(&property)
                .with_context(|| format!("undeclared property {property}"))?;
            let previous = component.get_property(&property)?;
            let mut value = self
                .convert(json, &previous, package)
                .with_context(|| format!("property {property}"))?;
            if value.value_type() != *declared {
                bail!(
                    "property {property}: declared {declared:?}, got {:?}",
                    value.value_type()
                );
            }
            coerce_numbers(
                &mut value,
                detailed.get(&property).context("missing detailed type")?,
            )?;
            component.set_property(&property, value)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn uses_declared_int_float_and_nested_model_types() -> Result<()> {
        let compiler = slint_interpreter::Compiler::default();
        let result = pollster::block_on(
            compiler.build_from_source(
                r#"
            export struct Item { item-count: int, fraction: float }
            export component Surface inherits Window {
                in property <int> count;
                in property <float> fraction;
                in property <[Item]> items;
            }
        "#
                .into(),
                Default::default(),
            ),
        );
        assert!(
            !result.has_errors(),
            "{:?}",
            result.diagnostics().collect::<Vec<_>>()
        );
        let definition = result.component("Surface").unwrap();
        let types: HashMap<_, _> = definition.properties().collect();
        assert_eq!(types.get("count"), Some(&ValueType::Number));
        assert_eq!(types.get("fraction"), Some(&ValueType::Number));
        let detailed: HashMap<_, _> = definition
            .properties_and_callbacks()
            .map(|(name, (ty, _))| (name, ty))
            .collect();
        let mut integer = Value::Number(4.9);
        coerce_numbers(&mut integer, &detailed["count"])?;
        assert_eq!(integer, Value::Number(4.0));
        let mut float = Value::Number(4.5);
        coerce_numbers(&mut float, &detailed["fraction"])?;
        assert_eq!(float, Value::Number(4.5));
        let mut items = Converter::default().convert(
            &json!([{"itemCount":3.9,"fraction":1.5}]),
            &Value::Void,
            Path::new("."),
        )?;
        coerce_numbers(&mut items, &detailed["items"])?;
        let Value::Model(items) = items else {
            panic!("model")
        };
        let Some(Value::Struct(item)) = items.row_data(0) else {
            panic!("item")
        };
        assert_eq!(item.get_field("item-count"), Some(&Value::Number(3.0)));
        assert_eq!(item.get_field("fraction"), Some(&Value::Number(1.5)));
        assert!(coerce_numbers(&mut Value::Number(f64::MAX), &Type::Int32).is_err());
        Ok(())
    }

    #[test]
    fn converts_nested_structs_models_and_preserves_null() -> Result<()> {
        let mut converter = Converter::default();
        let prior = Value::Struct(
            [("keep-me".into(), Value::Number(7.0))]
                .into_iter()
                .collect(),
        );
        let value = converter.convert(
            &json!({"keepMe":null,"someItems":[{"someText":"yes","enabled":true},2.5]}),
            &prior,
            Path::new("."),
        )?;
        let Value::Struct(value) = value else {
            panic!("struct")
        };
        assert_eq!(value.get_field("keep-me"), Some(&Value::Number(7.0)));
        let Some(Value::Model(items)) = value.get_field("some-items") else {
            panic!("model")
        };
        assert_eq!(items.row_count(), 2);
        assert_eq!(items.row_data(1), Some(Value::Number(2.5)));
        let Some(Value::Struct(item)) = items.row_data(0) else {
            panic!("struct item")
        };
        assert_eq!(
            item.get_field("some-text"),
            Some(&Value::String("yes".into()))
        );
        assert_eq!(item.get_field("enabled"), Some(&Value::Bool(true)));
        Ok(())
    }
    #[test]
    fn decodes_percent_encoded_svg_data_urls() -> Result<()> {
        let mut converter = Converter::default();
        let svg = json!({"$image":"data:image/svg+xml,%3Csvg%20xmlns=%22http://www.w3.org/2000/svg%22%20width=%222%22%20height=%223%22%3E%3Crect%20width=%222%22%20height=%223%22%20fill=%22red%22/%3E%3C/svg%3E"});
        let Value::Image(image) = converter.convert(&svg, &Value::Void, Path::new("."))? else {
            panic!("image")
        };
        assert_eq!((image.size().width, image.size().height), (2, 3));
        Ok(())
    }

    #[test]
    fn caches_file_images_after_the_first_decode() -> Result<()> {
        let model: Json = serde_json::from_str(include_str!("../fixtures/model.json"))?;
        let source = model["catalog"]["games"][0]["coverArtUrl"]
            .as_str()
            .unwrap();
        let (_, data) = source.split_once(',').unwrap();
        let path =
            std::env::temp_dir().join(format!("surface-shell-image-{}.png", std::process::id()));
        std::fs::write(
            &path,
            base64::engine::general_purpose::STANDARD.decode(data)?,
        )?;
        let mut converter = Converter::default();
        let json = json!({"$image":path.to_str().unwrap()});
        let first = converter.convert(&json, &Value::Void, Path::new("."))?;
        std::fs::remove_file(&path)?;
        assert_eq!(
            first,
            converter.convert(&json, &Value::Void, Path::new("."))?
        );
        assert_eq!(converter.images.len(), 1);
        Ok(())
    }

    #[test]
    fn decodes_data_url_once_and_rejects_invalid_images() -> Result<()> {
        let model: Json = serde_json::from_str(include_str!("../fixtures/model.json"))?;
        let source = model["catalog"]["games"][0]["coverArtUrl"]
            .as_str()
            .unwrap();
        let mut converter = Converter::default();
        let json = json!({"$image": source});
        let first = converter.convert(&json, &Value::Void, Path::new("."))?;
        assert_eq!(first.value_type(), ValueType::Image);
        assert_eq!(
            first,
            converter.convert(&json, &Value::Void, Path::new("."))?
        );
        assert_eq!(converter.images.len(), 1);
        assert!(converter
            .convert(
                &json!({"$image":"data:image/png;base64,!!!"}),
                &Value::Void,
                Path::new(".")
            )
            .is_err());
        Ok(())
    }
}
