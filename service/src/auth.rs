use crate::{
    config::Config,
    error::{Result, ServiceError},
};
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Arc};
use uuid::Uuid;

pub const SCOPES: &[&str] = &[
    "trust:observe",
    "trust:read",
    "trust:preview",
    "trust:trade",
    "trust:reconcile",
    "trust:manual",
    "trust:journal",
    "trust:policy",
    "trust:admin",
];

pub fn owner_scope(scope: &str) -> bool {
    matches!(scope, "trust:policy" | "trust:admin" | "trust:manual")
}

/// Operator-assigned identity class, never inferred from token scopes or claims.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    #[default]
    Agent,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    #[serde(default)]
    pub role: Role,
    #[serde(default)]
    pub accounts: BTreeSet<Uuid>,
    #[serde(default)]
    pub all_accounts: bool,
    pub scopes: BTreeSet<String>,
    #[serde(default)]
    pub allow_live: bool,
}

impl Grant {
    pub fn validate(&self) -> Result<()> {
        if self.role == Role::Agent && self.scopes.iter().any(|scope| owner_scope(scope)) {
            return Err(ServiceError::invalid(
                "Agent grants cannot include policy, administration or manual-accounting scopes.",
            ));
        }
        if self
            .scopes
            .iter()
            .any(|scope| !SCOPES.contains(&scope.as_str()))
            || (!self.all_accounts && self.accounts.is_empty())
        {
            return Err(ServiceError::invalid(
                "Grants require recognized scopes and explicit accounts or all_accounts.",
            ));
        }
        Ok(())
    }
}

/// Constructed only from verified tokens by HTTP middleware; never from operation input.
#[derive(Clone, Debug)]
pub struct Identity {
    pub subject: String,
    pub client_id: Option<String>,
    pub expires_at: u64,
    pub scopes: BTreeSet<String>,
    pub grant: Grant,
    /// Derived only from verified issuer authentication evidence. Refreshing a
    /// token does not extend the human's original authentication time.
    pub owner_authenticated_until: Option<u64>,
}

impl Identity {
    pub fn permits(&self, scope: &str) -> bool {
        self.scopes.contains(scope)
            && self.grant.scopes.contains(scope)
            && (!owner_scope(scope) || self.grant.role == Role::Owner)
    }
    pub fn permits_account(&self, account: Uuid) -> bool {
        self.grant.all_accounts || self.grant.accounts.contains(&account)
    }
    pub fn check(&self, scope: &str, account: Option<Uuid>) -> Result<()> {
        let now =
            u64::try_from(chrono::Utc::now().timestamp()).map_err(|_| ServiceError::forbidden())?;
        if self.expires_at <= now
            || !self.permits(scope)
            || account.is_some_and(|id| !self.permits_account(id))
        {
            return Err(ServiceError::forbidden());
        }
        if owner_scope(scope) {
            self.check_owner_authentication(now)?;
        }
        Ok(())
    }

    fn check_owner_authentication(&self, now: u64) -> Result<()> {
        if self
            .owner_authenticated_until
            .is_none_or(|until| until <= now)
        {
            return Err(ServiceError::new(
                "insufficient_user_authentication",
                "This owner action requires recent human authentication with MFA.",
                "Sign in again at the configured OAuth provider using the owner's MFA policy. Keep owner credentials out of agent sessions; retry the original intent with the same idempotency key.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct Authenticator {
    config: Arc<Config>,
    keys: Arc<JwkSet>,
}

#[derive(Debug, Deserialize)]
struct Claims {
    sub: String,
    exp: u64,
    iat: u64,
    #[serde(default)]
    scope: String,
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    azp: Option<String>,
    #[serde(default)]
    jti: Option<String>,
    #[serde(default)]
    auth_time: Option<u64>,
    #[serde(default)]
    acr: Option<String>,
}

impl Authenticator {
    pub fn new(config: Arc<Config>) -> Result<Self> {
        let bytes = std::fs::read(&config.jwks_file)
            .map_err(|_| ServiceError::invalid("Cannot read the pinned public JWKS file."))?;
        let keys: JwkSet = serde_json::from_slice(&bytes)
            .map_err(|_| ServiceError::invalid("Invalid public JWKS file."))?;
        if keys.keys.is_empty() || keys.keys.len() > 32 {
            return Err(ServiceError::invalid(
                "JWKS must contain 1..32 verification keys.",
            ));
        }
        Ok(Self {
            config,
            keys: Arc::new(keys),
        })
    }

    pub fn authenticate(&self, token: &str) -> Result<Identity> {
        if token.len() > 16384 {
            return Err(unauthorized());
        }
        let header = decode_header(token).map_err(|_| unauthorized())?;
        if header.alg != Algorithm::RS256 {
            return Err(unauthorized());
        }
        let kid = header.kid.as_deref().ok_or_else(unauthorized)?;
        let jwk = self.keys.find(kid).ok_or_else(unauthorized)?;
        let key = DecodingKey::from_jwk(jwk).map_err(|_| unauthorized())?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[&self.config.issuer]);
        validation.set_audience(&[self.config.resource()]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub", "iat"]);
        validation.validate_nbf = true;
        validation.leeway = 0;
        let claims = decode::<Claims>(token, &key, &validation)
            .map_err(|_| unauthorized())?
            .claims;
        let now = u64::try_from(chrono::Utc::now().timestamp()).map_err(|_| unauthorized())?;
        if claims.iat > now
            || claims.exp <= claims.iat
            || claims.exp.saturating_sub(claims.iat) > self.config.max_token_lifetime_seconds
            || claims
                .jti
                .as_ref()
                .is_some_and(|id| self.config.revoked_token_ids.contains(id))
        {
            return Err(unauthorized());
        }
        let grant = self
            .config
            .grants
            .get(&claims.sub)
            .cloned()
            .ok_or_else(unauthorized)?;
        let owner_authenticated_until = claims
            .auth_time
            .filter(|_| grant.role == Role::Owner)
            .filter(|time| *time <= claims.iat && *time <= now)
            .filter(|_| {
                claims
                    .acr
                    .as_ref()
                    .is_some_and(|acr| self.config.owner_authentication.acr_values.contains(acr))
            })
            .and_then(|time| time.checked_add(self.config.owner_authentication.max_age_seconds))
            .map(|until| until.min(claims.exp));
        let identity = Identity {
            subject: claims.sub,
            client_id: claims.client_id.or(claims.azp),
            expires_at: claims.exp,
            scopes: claims.scope.split_whitespace().map(str::to_owned).collect(),
            grant,
            owner_authenticated_until,
        };
        // A broad owner token is an elevated session. Challenge at the HTTP
        // boundary as well as before dispatch, including requests queued earlier.
        if identity.grant.role == Role::Owner
            && identity
                .scopes
                .iter()
                .any(|scope| owner_scope(scope) && identity.permits(scope))
        {
            identity.check_owner_authentication(now)?;
        }
        Ok(identity)
    }
}

pub fn unauthorized() -> ServiceError {
    ServiceError::new(
        "unauthorized",
        "A valid Trust access token is required.",
        "Authenticate with the configured authorization server for the Trust resource.",
    )
}
