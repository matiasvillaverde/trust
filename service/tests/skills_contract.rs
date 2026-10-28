#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::Path};
use trust_service::{
    auth::{Grant, Identity, Role},
    catalog, skills,
};

fn identity(owner: bool) -> Identity {
    let scopes: BTreeSet<String> = if owner {
        vec!["trust:read", "trust:admin"]
    } else {
        vec!["trust:read"]
    }
    .into_iter()
    .map(str::to_owned)
    .collect();
    Identity {
        subject: "skill-reader".into(),
        client_id: None,
        expires_at: u64::MAX,
        scopes: scopes.clone(),
        owner_authenticated_until: Some(u64::MAX),
        grant: Grant {
            role: if owner { Role::Owner } else { Role::Agent },
            all_accounts: true,
            accounts: BTreeSet::new(),
            scopes,
            allow_live: false,
        },
    }
}

#[test]
fn embedded_files_match_source_manifest_links_and_tool_examples() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../skills");
    let owner = identity(true);
    let mut mentioned = BTreeSet::new();
    let mut example_keys = BTreeSet::new();
    let mut examples = 0;
    let mut files = BTreeSet::new();
    for skill in skills::catalogue() {
        let body = skills::read(&owner, &skill.name).unwrap();
        assert!(body.starts_with(&format!(
            "---\nname: {}\ndescription: {}\n---\n",
            skill.name, skill.description
        )));
        assert!(!skill.description.is_empty() && skill.description.len() <= 1024);
        assert_eq!(skills::read_uri(&owner, &skill.uri).unwrap().content, body);
        assert_eq!(
            skills::read_uri(&owner, &skill.entrypoint).unwrap().content,
            body
        );
        for file in &skill.files {
            let disk = root.join(&skill.name).join(&file.path);
            assert!(!disk.is_symlink());
            let bytes = std::fs::read(&disk).unwrap();
            let digest: String = Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            assert_eq!(file.sha256, digest);
            assert_eq!(file.size_bytes, bytes.len());
            let served = skills::read_file(&owner, &skill.name, &file.path).unwrap();
            assert_eq!(served.content.as_bytes(), bytes);
            assert_eq!(served.sha256, digest);
            assert_eq!(
                skills::read_uri(&owner, &file.uri).unwrap().content,
                served.content
            );
            assert_eq!(served.version, skill.version);
            assert!(files.insert(format!("{}/{}", skill.name, file.path)));
            if !file.path.ends_with(".md") {
                continue;
            }
            for tool in catalog::tools() {
                if served.content.contains(&format!("`{}`", tool.name)) {
                    mentioned.insert(tool.name);
                }
            }
            for link in served.content.split("](").skip(1) {
                let target = link.split(')').next().unwrap();
                if target.starts_with("https://") || target.starts_with('#') {
                    continue;
                }
                let path = Path::new(&file.path).parent().unwrap().join(target);
                assert!(
                    skills::read_file(&owner, &skill.name, path.to_str().unwrap()).is_ok(),
                    "Broken bundled reference: {} -> {target}",
                    file.path
                );
            }
            for block in served.content.split("```json\n").skip(1) {
                let example: Value =
                    serde_json::from_str(block.split("```").next().unwrap()).unwrap();
                let name = example["tool"].as_str().unwrap();
                let tool = catalog::find(name).unwrap();
                let args = &example["arguments"];
                catalog::validate(tool, args)
                    .unwrap_or_else(|error| panic!("Invalid {name} example: {error:?}"));
                jsonschema::validator_for(&tool.input_schema)
                    .unwrap()
                    .validate(args)
                    .unwrap();
                if tool.mutation {
                    assert!(
                        example_keys.insert(args["idempotency_key"].as_str().unwrap().to_owned()),
                        "Example reused an intent key"
                    );
                }
                examples += 1;
            }
        }
    }
    fn walk(root: &Path, directory: &Path, files: &mut BTreeSet<String>) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            assert!(!path.is_symlink());
            if path.is_dir() {
                walk(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_owned(),
                );
            }
        }
    }
    let mut disk_files = BTreeSet::new();
    walk(&root, &root, &mut disk_files);
    assert_eq!(
        files, disk_files,
        "Every shipped skill file must be available through MCP too"
    );
    assert!(examples >= 10, "Worked tool calls must remain covered");
    let missing: Vec<_> = catalog::tools()
        .iter()
        .filter(|tool| !mentioned.contains(tool.name))
        .map(|tool| tool.name)
        .collect();
    assert!(
        missing.is_empty(),
        "Tools need workflow routing: {missing:?}"
    );
}

#[test]
fn skill_files_do_not_escape_the_bundle_or_caller_permissions() {
    let agent = identity(false);
    let list = skills::list(&agent);
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert!(list
        .as_array()
        .unwrap()
        .iter()
        .all(|skill| skill["name"] != "trust-admin"));
    for file in &skills::catalogue()
        .iter()
        .find(|skill| skill.name == "trust-admin")
        .unwrap()
        .files
    {
        assert!(skills::read_file(&agent, "trust-admin", &file.path).is_err());
        assert!(skills::read_uri(&agent, &file.uri).is_err());
    }
    for path in [
        "../trust-admin/SKILL.md",
        "references/../../trust-admin/SKILL.md",
        "/etc/passwd",
        "references//recovery.md",
        "references/../SKILL.md",
        "%2e%2e/trust-admin/SKILL.md",
        "SKILL.md?token=private",
        "SKILL.md#section",
        "SKILL.md\0",
        "references\\recovery.md",
    ] {
        assert!(skills::read_file(&agent, "trust-operations", path).is_err());
        assert!(
            skills::read_uri(&agent, &format!("skill://trust/trust-operations/{path}")).is_err()
        );
    }
    for uri in [
        "file:///etc/passwd",
        "skill://evil/trust-risk/SKILL.md",
        "skill://trust/trust-risk",
        "trust://skills/trust-admin",
        "https://trust/skills/trust-risk/SKILL.md",
    ] {
        assert!(skills::read_uri(&agent, uri).is_err());
    }
    let mut forged_agent = agent.clone();
    forged_agent.scopes.insert("trust:admin".into());
    forged_agent.grant.scopes.insert("trust:admin".into());
    assert!(skills::read(&forged_agent, "trust-admin").is_err());
    let mut expired = agent;
    expired.expires_at = 0;
    assert!(skills::read(&expired, "trust-risk").is_err());
}

#[test]
fn plugin_packages_match_official_portable_schemas() {
    // Unmodified Agent Plugins 1.0.0 schemas, retrieved from
    // https://agent-plugins.org/schemas/1.0.0/{plugin,mcp}.schema.json.
    // Keep validation offline. In particular, portable MCP entries reject
    // client-specific auth extensions even where a host documents its own variant.
    let schemas = [
        (
            "plugin",
            include_str!("fixtures/agent-plugins/plugin.schema.json"),
        ),
        (
            "mcp",
            include_str!("fixtures/agent-plugins/mcp.schema.json"),
        ),
    ];
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let directory = tempfile::tempdir().unwrap();
    for audience in ["agent", "owner"] {
        let output = directory.path().join(audience);
        let result = std::process::Command::new("python3")
            .arg(root.join("scripts/package_skills.py"))
            .args([
                "--mcp-url",
                "https://trust.example.com/mcp",
                "--audience",
                audience,
                "--output",
            ])
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        for (name, schema) in schemas {
            let document: Value = serde_json::from_slice(
                &std::fs::read(output.join(format!("{name}.json"))).unwrap(),
            )
            .unwrap();
            let schema: Value = serde_json::from_str(schema).unwrap();
            jsonschema::validator_for(&schema)
                .unwrap()
                .validate(&document)
                .unwrap();
        }
    }
}
