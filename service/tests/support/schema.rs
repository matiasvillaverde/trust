use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock};

pub fn validate(name: &str, reply: &Value) {
    static VALIDATORS: OnceLock<BTreeMap<&'static str, jsonschema::Validator>> = OnceLock::new();
    let validators = VALIDATORS.get_or_init(|| {
        trust_service::catalog::tools()
            .iter()
            .map(|spec| {
                let schema = Value::Object((*trust_service::output_schema::output(spec)).clone());
                (
                    spec.name,
                    jsonschema::validator_for(&schema)
                        .unwrap_or_else(|error| panic!("{} schema: {error}", spec.name)),
                )
            })
            .collect()
    });
    if let Some(validator) = validators.get(name) {
        let errors: Vec<_> = validator
            .iter_errors(reply)
            .map(|error| format!("{}: {error}", error.instance_path()))
            .collect();
        assert!(
            errors.is_empty(),
            "{name} result violated its output schema: {}",
            errors.join("; ")
        );
    }
}
