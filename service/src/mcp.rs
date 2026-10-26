//! MCP is a transport of the application service, never a second trading engine.
use crate::{
    auth::Identity,
    catalog::{self, ToolSpec},
    runtime::Service,
    skills,
};
use rmcp::{model::*, service::RequestContext, ErrorData as McpError, RoleServer, ServerHandler};
use serde_json::{json, Value};
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
fn tool(spec: ToolSpec) -> Tool {
    let schema = spec.input_schema.as_object().cloned().unwrap_or_default();
    Tool::new(spec.name,spec.description,schema)
        .with_annotations(ToolAnnotations::new().read_only(!spec.mutation).destructive(spec.mutation).idempotent(true).open_world(true))
        .with_raw_output_schema(std::sync::Arc::new(json!({"type":"object","required":["schema_version","request_id","data","error","next_actions"],"properties":{"schema_version":{"type":"integer"},"request_id":{"type":"string"},"operation_id":{"type":["string","null"]},"data":{},"error":{"type":["object","null"]},"next_actions":{"type":"array","items":{"type":"string"}}}}).as_object().cloned().unwrap_or_default()))
}
impl ServerHandler for Mcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().enable_resources().enable_prompts().build())
            .with_server_info(Implementation::new("trust",env!("CARGO_PKG_VERSION")))
            .with_instructions("Start with service_status and policy_get. Trust is the risk and ledger authority. Read trust://skills/trust-risk. All mutations require stable idempotency keys. Never infer a fill from a submitted order or retry uncertain execution with a new key.")
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
                .into_iter()
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
            .call(
                identity,
                request.name.into_owned(),
                Value::Object(request.arguments.unwrap_or_default()),
                request_id,
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
            skills::NAMES
                .iter()
                .filter(|name| skills::visible(&identity, name))
                .map(|name| {
                    Resource::new(format!("trust://skills/{name}"), *name)
                        .with_mime_type("text/markdown")
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
        let name = request
            .uri
            .strip_prefix("trust://skills/")
            .ok_or_else(|| McpError::invalid_params("Unknown Trust resource", None))?;
        let content = skills::read(&identity, name).map_err(|_| {
            McpError::invalid_params("Resource unavailable for this identity", None)
        })?;
        Ok(
            ReadResourceResult::new(vec![ResourceContents::text(content, request.uri)])
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
            skills::NAMES
                .iter()
                .filter(|name| skills::visible(&identity, name))
                .map(|name| Prompt::new(*name, Some(format!("Follow the {name} workflow")), None))
                .collect(),
        ))
    }
    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, McpError> {
        let identity = identity(&context)?;
        let content = skills::read(&identity, &request.name)
            .map_err(|_| McpError::invalid_params("Prompt unavailable for this identity", None))?;
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(Role::User, content)]).into())
    }
}
