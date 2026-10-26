use crate::{
    auth::Identity,
    error::{Result, ServiceError},
};
use serde_json::{json, Value};
pub const NAMES: &[&str] = &["trust-risk", "trust-operations", "trust-admin"];
pub fn visible(identity: &Identity, name: &str) -> bool {
    identity.permits("trust:read") && (name != "trust-admin" || identity.permits("trust:admin"))
}
pub fn list(identity: &Identity) -> Value {
    json!(NAMES
        .iter()
        .filter(|name| visible(identity, name))
        .map(|name| json!({"name":name,"uri":format!("trust://skills/{name}")}))
        .collect::<Vec<_>>())
}
pub fn read(identity: &Identity, name: &str) -> Result<&'static str> {
    identity.check("trust:read", None)?;
    if !visible(identity, name) {
        return Err(ServiceError::forbidden());
    }
    match name {
        "trust-risk" => Ok(include_str!("../../skills/trust-risk/SKILL.md")),
        "trust-operations" => Ok(include_str!("../../skills/trust-operations/SKILL.md")),
        "trust-admin" => Ok(include_str!("../../skills/trust-admin/SKILL.md")),
        _ => Err(ServiceError::invalid("Unknown Trust skill.")),
    }
}
