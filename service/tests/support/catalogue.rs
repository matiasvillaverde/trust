//! Valid catalogue requests for authorization and input-boundary contracts.
use serde_json::{json, Value};

pub(super) fn schema_sample(schema: &Value) -> Value {
    if let Some(constant) = schema.get("const") {
        return constant.clone();
    }
    if let Some(first) = schema
        .get("enum")
        .and_then(Value::as_array)
        .and_then(|v| v.first())
    {
        return first.clone();
    }
    match schema["type"].as_str().unwrap() {
        "string" => match schema["format"].as_str() {
            Some("uuid") => json!("00000000-0000-0000-0000-000000000001"),
            Some("decimal") => json!("1.25"),
            Some("date-time") => json!("2099-01-01T00:00:00Z"),
            _ => json!("test-value"),
        },
        "integer" => schema.get("minimum").cloned().unwrap_or(json!(0)),
        "boolean" => json!(false),
        "array" => json!([schema_sample(&schema["items"])]),
        "object" => Value::Object(
            schema
                .get("properties")
                .and_then(Value::as_object)
                .map(|props| {
                    props
                        .iter()
                        .map(|(k, v)| (k.clone(), schema_sample(v)))
                        .collect()
                })
                .unwrap_or_default(),
        ),
        other => panic!("new schema kind needs a boundary fixture: {other}"),
    }
}
pub(super) fn tool_arguments(tool: &trust_service::catalog::ToolSpec) -> Value {
    let required = tool.input_schema["required"].as_array().unwrap();
    Value::Object(
        tool.input_schema["properties"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(name, _)| required.contains(&json!(name)))
            .map(|(name, schema)| (name.clone(), schema_sample(schema)))
            .collect(),
    )
}

#[test]
fn every_tool_rejects_missing_unknown_wrong_type_and_boundary_arguments() {
    use trust_service::catalog::{tools, validate};
    let mut checks = 0_u32;
    for tool in tools() {
        let base = tool_arguments(tool);
        validate(tool, &base).unwrap_or_else(|e| panic!("{}: {e:?}", tool.name));
        for required in tool.input_schema["required"].as_array().unwrap() {
            let mut args = base.clone();
            args.as_object_mut()
                .unwrap()
                .remove(required.as_str().unwrap());
            assert!(
                validate(tool, &args).is_err(),
                "{} missing {required}",
                tool.name
            );
            checks += 1;
        }
        for (name, schema) in tool.input_schema["properties"].as_object().unwrap() {
            let mut args = base.clone();
            args[name] = schema_sample(schema);
            assert!(validate(tool, &args).is_ok(), "{} valid {name}", tool.name);
            let mut invalid = vec![Value::Null, json!({})];
            match schema["type"].as_str().unwrap() {
                "string" => {
                    invalid.extend([
                        json!(1),
                        json!(true),
                        json!("a\0b"),
                        json!("a".repeat(
                            usize::try_from(schema["maxLength"].as_u64().unwrap_or(2000)).unwrap()
                                + 1
                        )),
                    ]);
                    if schema["minLength"].as_u64().is_some_and(|n| n > 0) {
                        invalid.push(json!(""));
                    }
                    if schema.get("enum").is_some() {
                        invalid.push(json!("not-an-enum-value"));
                    }
                    match schema["format"].as_str() {
                        Some("uuid") => invalid.extend([json!("not-a-uuid"), json!("../private")]),
                        Some("decimal") => invalid.extend([
                            json!("NaN"),
                            json!("Infinity"),
                            json!("1e2"),
                            json!("79228162514264337593543950336"),
                            json!("0.00000000000000000000000000001"),
                            json!("1.12345678901234567890123456789"),
                        ]),
                        _ => {}
                    }
                }
                "integer" => {
                    invalid.extend([json!("1"), json!(-1), json!(1.25), json!(true)]);
                    if let Some(min) = schema["minimum"].as_u64().filter(|n| *n > 0) {
                        invalid.push(json!(min - 1));
                    }
                    if let Some(max) = schema["maximum"].as_u64().and_then(|n| n.checked_add(1)) {
                        invalid.push(json!(max));
                    }
                }
                "boolean" => invalid.extend([json!("false"), json!(0)]),
                "array" => invalid.extend([
                    json!("item"),
                    json!([null]),
                    json!([{}]),
                    json!(vec!["a"; 65]),
                ]),
                "object" => invalid.extend([json!(1), json!("object"), json!({"unexpected":true})]),
                _ => unreachable!(),
            }
            for value in invalid {
                args[name] = value;
                assert!(
                    validate(tool, &args).is_err(),
                    "{} accepted invalid {name}: {}",
                    tool.name,
                    args[name]
                );
                checks += 1;
            }
        }
        let mut unknown = base;
        unknown["unexpected_argument"] = json!(true);
        assert!(validate(tool, &unknown).is_err());
        for value in [json!(null), json!([]), json!(1), json!("arguments")] {
            assert!(validate(tool, &value).is_err());
        }
    }
    assert!(
        checks > 2000,
        "catalogue boundary checks unexpectedly disappeared: {checks}"
    );
    println!("Catalogue boundary assertions: {checks}");
}
