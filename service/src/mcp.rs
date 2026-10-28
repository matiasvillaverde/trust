//! MCP is a transport of the application service, never a second trading engine.
use crate::{
    auth::Identity,
    catalog::{self, ToolSpec},
    runtime::Service,
    skills,
};
use rmcp::{model::*, service::RequestContext, ErrorData as McpError, RoleServer, ServerHandler};
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone)]
pub struct Mcp {
    pub service: Service,
}
fn identity(context: &RequestContext<RoleServer>) -> Result<Identity, McpError> {
    context
        .extensions
        .get::<axum::http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<Identity>())
        .cloned()
        .ok_or_else(|| {
            McpError::invalid_request("Authenticated request identity is required", None)
        })
}
pub fn visible(identity: &Identity, tool: &ToolSpec) -> bool {
    identity.permits(tool.scope)
        && !(tool.scope == "trust:admin" && !tool.account_required && !identity.grant.all_accounts)
}
fn tool(spec: &ToolSpec) -> Tool {
    let schema = spec.input_schema.as_object().cloned().unwrap_or_default();
    let description = if spec.owner_only {
        format!(
            "Owner only; recent human MFA required. {}",
            spec.description
        )
    } else {
        spec.description.to_owned()
    };
    Tool::new(spec.name, description, schema)
        .with_annotations(
            ToolAnnotations::new()
                .read_only(!spec.mutation)
                .destructive(spec.mutation)
                .idempotent(true)
                .open_world(true),
        )
        .with_raw_output_schema(crate::output_schema::output(spec))
}
impl ServerHandler for Mcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().enable_resources().enable_prompts().build())
            .with_server_info(Implementation::new("trust",env!("CARGO_PKG_VERSION")))
            .with_instructions("Discover skills_list and load the relevant workflow with skill_read or its resource URI: trust-risk for market research, portfolio analysis and sizing/new entries, trust-operations for existing trades and uncertainty, trust-admin for human owner setup/recovery. Agents asked for owner changes use trust-risk's references/owner-handoff.md without elevating credentials. Read supporting files only as needed using skill_read(name,path) or resources/read. Establish context with service_status, policy_get and accounts_list; resolve account ambiguity before any mutation. Use account_readiness for financial decisions. Research stays in its read workflow; analysis or sizing does not authorize draft/fund/submit. Check actions[].available and blockers; required_arguments lists missing inputs. Trust rechecks policy, fresh broker exposure and your mandate at funding and submission. Mutations require stable keys. Never infer a fill from submission or repeat uncertain execution with a new key. Owner credentials belong only in the human's separate session.")
    }
    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let identity = identity(&context)?;
        if request.and_then(|r| r.cursor).is_some() {
            return Err(McpError::invalid_params(
                "This tool catalogue fits in one page",
                None,
            ));
        }
        Ok(ListToolsResult::with_all_items(
            catalog::tools()
                .iter()
                .filter(|t| visible(&identity, t))
                .map(tool)
                .collect(),
        ))
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        catalog::find(name).ok().map(tool)
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let identity = identity(&context)?;
        let request_id = context
            .extensions
            .get::<axum::http::request::Parts>()
            .and_then(|parts| parts.extensions.get::<Uuid>())
            .copied()
            .unwrap_or_else(Uuid::new_v4);
        let reply = self
            .service
            .call_from(
                identity,
                request.name.into_owned(),
                Value::Object(request.arguments.unwrap_or_default()),
                request_id,
                crate::runtime::Source::Mcp,
            )
            .await;
        let failed = reply.error.is_some();
        let value = serde_json::to_value(reply)
            .map_err(|_| McpError::internal_error("Cannot encode result", None))?;
        Ok(if failed {
            CallToolResult::structured_error(value)
        } else {
            CallToolResult::structured(value)
        }
        .into())
    }
    async fn list_resources(
        &self,
        _: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let identity = identity(&context)?;
        Ok(ListResourcesResult::with_all_items(
            skills::catalogue()
                .iter()
                .filter(|skill| skills::visible(&identity, &skill.name))
                .flat_map(|skill| {
                    let alias = Resource::new(skill.uri.clone(), skill.name.clone())
                        .with_description(skill.description.clone())
                        .with_mime_type("text/markdown");
                    std::iter::once(alias).chain(skill.files.iter().map(|file| {
                        Resource::new(file.uri.clone(), format!("{}/{}", skill.name, file.path))
                            .with_description(skill.description.clone())
                            .with_mime_type(file.mime_type.clone())
                    }))
                })
                .collect(),
        ))
    }
    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let identity = identity(&context)?;
        let file = skills::read_uri(&identity, &request.uri).map_err(|_| {
            McpError::invalid_params("Resource unavailable for this identity", None)
        })?;
        Ok(
            ReadResourceResult::new(vec![ResourceContents::text(file.content, request.uri)
                .with_mime_type(if file.path.ends_with(".md") {
                    "text/markdown"
                } else {
                    "text/yaml"
                })])
            .with_ttl_ms(0)
            .with_cache_scope(CacheScope::Private)
            .into(),
        )
    }
    async fn list_prompts(
        &self,
        _: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        let identity = identity(&context)?;
        Ok(ListPromptsResult::with_all_items(
            skills::catalogue()
                .iter()
                .filter(|skill| skills::visible(&identity, &skill.name))
                .map(|skill| {
                    Prompt::new(
                        skill.name.clone(),
                        Some(skill.description.clone()),
                        Some(vec![
                            PromptArgument::new("account_id")
                                .with_description(
                                    "Permitted Trust account UUID; omitted for discovery.",
                                )
                                .with_required(false),
                            PromptArgument::new("trade_id")
                                .with_description("Trade UUID; requires account_id.")
                                .with_required(false),
                        ]),
                    )
                })
                .collect(),
        ))
    }
    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, McpError> {
        let identity = identity(&context)?;
        let content = skills::prompt(
            &identity,
            &request.name,
            request.arguments.unwrap_or_default(),
        )
        .map_err(|_| McpError::invalid_params("Prompt unavailable for this identity", None))?;
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(Role::User, content)]).into())
    }
}
