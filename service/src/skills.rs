//! One embedded skill catalogue for MCP resources, prompts, tools and package checks.
//! Reads are exact allowlist lookups; a caller-controlled path never reaches the filesystem.
use crate::{
    auth::Identity,
    error::{Result, ServiceError},
};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

pub struct SkillFile {
    pub path: &'static str,
    pub content: &'static str,
}
pub struct Skill {
    pub name: &'static str,
    pub files: &'static [SkillFile],
}
macro_rules! file {
    ($name:literal, $path:literal) => {
        SkillFile {
            path: $path,
            content: include_str!(concat!("../../skills/", $name, "/", $path)),
        }
    };
}

pub const SKILLS: &[Skill] = &[
    Skill {
        name: "trust-risk",
        files: &[
            file!("trust-risk", "SKILL.md"),
            file!("trust-risk", "references/trade-entry.md"),
            file!("trust-risk", "references/analysis.md"),
            file!("trust-risk", "references/owner-handoff.md"),
            file!("trust-risk", "agents/openai.yaml"),
        ],
    },
    Skill {
        name: "trust-operations",
        files: &[
            file!("trust-operations", "SKILL.md"),
            file!("trust-operations", "references/recovery.md"),
            file!("trust-operations", "references/protection-and-exits.md"),
            file!("trust-operations", "references/review.md"),
            file!("trust-operations", "agents/openai.yaml"),
        ],
    },
    Skill {
        name: "trust-admin",
        files: &[
            file!("trust-admin", "SKILL.md"),
            file!("trust-admin", "references/setup-and-policy.md"),
            file!("trust-admin", "references/recovery-and-audit.md"),
            file!("trust-admin", "agents/openai.yaml"),
        ],
    },
];

/// Input-schema allowlist; name/path pairing is checked again by read_file.
pub fn paths() -> &'static [&'static str] {
    static PATHS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
        SKILLS
            .iter()
            .flat_map(|skill| skill.files.iter().map(|file| file.path))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    });
    &PATHS
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileMetadata {
    pub path: String,
    pub uri: String,
    pub mime_type: String,
    pub sha256: String,
    pub size_bytes: usize,
}
#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    pub version: String,
    /// Existing clients may keep using the original entrypoint alias.
    pub uri: String,
    pub entrypoint: String,
    pub files: Vec<FileMetadata>,
}
#[derive(Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SkillContent {
    pub name: String,
    pub path: String,
    pub uri: String,
    pub version: String,
    pub sha256: String,
    pub content: String,
}

/// The checked-in frontmatter uses plain single-line descriptions. CI validates this
/// convention and its parity with the skill directory name and packaged metadata.
fn description(skill: &Skill) -> &str {
    skill
        .files
        .iter()
        .find(|file| file.path == "SKILL.md")
        .and_then(|file| file.content.strip_prefix("---\n"))
        .and_then(|content| content.split_once("\n---"))
        .and_then(|(frontmatter, _)| {
            frontmatter
                .lines()
                .find_map(|line| line.strip_prefix("description: "))
        })
        .unwrap_or(skill.name)
}

pub fn catalogue() -> &'static [SkillMetadata] {
    static CATALOGUE: LazyLock<Vec<SkillMetadata>> = LazyLock::new(|| {
        SKILLS
            .iter()
            .map(|skill| SkillMetadata {
                name: skill.name.into(),
                description: description(skill).into(),
                version: env!("CARGO_PKG_VERSION").into(),
                uri: format!("trust://skills/{}", skill.name),
                entrypoint: format!("skill://trust/{}/SKILL.md", skill.name),
                files: skill
                    .files
                    .iter()
                    .map(|file| FileMetadata {
                        path: file.path.into(),
                        uri: format!("skill://trust/{}/{}", skill.name, file.path),
                        mime_type: if file.path.ends_with(".md") {
                            "text/markdown"
                        } else {
                            "text/yaml"
                        }
                        .into(),
                        sha256: Sha256::digest(file.content.as_bytes())
                            .iter()
                            .map(|byte| format!("{byte:02x}"))
                            .collect(),
                        size_bytes: file.content.len(),
                    })
                    .collect(),
            })
            .collect()
    });
    &CATALOGUE
}

pub fn visible(identity: &Identity, name: &str) -> bool {
    identity.permits("trust:read") && (name != "trust-admin" || identity.permits("trust:admin"))
}
pub fn list(identity: &Identity) -> Value {
    json!(catalogue()
        .iter()
        .filter(|skill| visible(identity, &skill.name))
        .collect::<Vec<_>>())
}
fn file_content(identity: &Identity, name: &str, path: &str) -> Result<&'static str> {
    identity.check("trust:read", None)?;
    if !visible(identity, name) {
        return Err(ServiceError::forbidden());
    }
    SKILLS
        .iter()
        .find(|skill| skill.name == name)
        .and_then(|skill| skill.files.iter().find(|file| file.path == path))
        .map(|file| file.content)
        .ok_or_else(|| ServiceError::invalid("Unknown bundled Trust skill file; use skills_list."))
}
pub fn read(identity: &Identity, name: &str) -> Result<&'static str> {
    file_content(identity, name, "SKILL.md")
}
pub fn read_file(identity: &Identity, name: &str, path: &str) -> Result<SkillContent> {
    let content = file_content(identity, name, path)?;
    let metadata = catalogue()
        .iter()
        .find(|skill| skill.name == name)
        .and_then(|skill| skill.files.iter().find(|file| file.path == path))
        .ok_or_else(ServiceError::internal)?;
    Ok(SkillContent {
        name: name.into(),
        path: path.into(),
        uri: metadata.uri.clone(),
        version: env!("CARGO_PKG_VERSION").into(),
        sha256: metadata.sha256.clone(),
        content: content.into(),
    })
}
pub fn read_uri(identity: &Identity, uri: &str) -> Result<SkillContent> {
    let relative = uri
        .strip_prefix("skill://trust/")
        .or_else(|| uri.strip_prefix("trust://skills/"))
        .ok_or_else(|| ServiceError::invalid("Unknown Trust skill resource."))?;
    let (name, path) = match relative.split_once('/') {
        Some(parts) => parts,
        None if uri.starts_with("trust://skills/") => (relative, "SKILL.md"),
        None => {
            return Err(ServiceError::invalid(
                "Skill resource must name a bundled file.",
            ))
        }
    };
    read_file(identity, name, path)
}

pub fn prompt(
    identity: &Identity,
    name: &str,
    args: serde_json::Map<String, Value>,
) -> Result<String> {
    let content = read(identity, name)?;
    if args
        .keys()
        .any(|key| !matches!(key.as_str(), "account_id" | "trade_id"))
    {
        return Err(ServiceError::invalid(
            "Prompt arguments are account_id and optional trade_id UUIDs.",
        ));
    }
    let mut context = String::new();
    if let Some(account) = args.get("account_id") {
        let account = account
            .as_str()
            .and_then(|id| id.parse::<uuid::Uuid>().ok())
            .ok_or_else(|| ServiceError::invalid("Invalid account_id."))?;
        identity.check("trust:read", Some(account))?;
        context.push_str(&format!("\n\nSelected account_id: `{account}`. Read account_readiness for this account before taking new risk."));
    }
    if let Some(trade) = args.get("trade_id") {
        if !args.contains_key("account_id") {
            return Err(ServiceError::invalid("trade_id requires account_id."));
        }
        let trade = trade
            .as_str()
            .and_then(|id| id.parse::<uuid::Uuid>().ok())
            .ok_or_else(|| ServiceError::invalid("Invalid trade_id."))?;
        context.push_str(&format!("\nSelected trade_id: `{trade}`. Read trade_get with both IDs to verify the trade and its current lifecycle."));
    }
    Ok(format!("{content}{context}"))
}
