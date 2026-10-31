use crate::{
    config::Config,
    error::{Result, ServiceError},
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::{
    decode, decode_header,
    jwk::{AlgorithmParameters, JwkSet, KeyAlgorithm, KeyOperations, PublicKeyUse},
    Algorithm, DecodingKey, Validation,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
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
    keys: Arc<BTreeMap<String, DecodingKey>>,
    validation: Arc<Validation>,
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
        let bytes = trust_security::read_public_config(&config.jwks_file, 1024 * 1024)
            .map_err(|_| ServiceError::invalid("Cannot read the pinned public JWKS file."))?;
        let document: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|_| ServiceError::invalid("Invalid public JWKS file."))?;
        // Pin only public verification material. Jwk's deserializer intentionally
        // ignores unknown members, including RSA private parameters.
        if document
            .get("keys")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|keys| {
                keys.iter().any(|key| {
                    ["d", "p", "q", "dp", "dq", "qi", "oth", "k"]
                        .iter()
                        .any(|field| key.get(field).is_some())
                })
            })
        {
            return Err(ServiceError::invalid(
                "JWKS must contain public keys only; remove private or symmetric key material.",
            ));
        }
        let keys: JwkSet = serde_json::from_value(document)
            .map_err(|_| ServiceError::invalid("Invalid public JWKS file."))?;
        if keys.keys.is_empty() || keys.keys.len() > 32 {
            return Err(ServiceError::invalid(
                "JWKS must contain 1..32 verification keys.",
            ));
        }
        let mut seen = BTreeSet::new();
        let mut verification_keys = BTreeMap::new();
        for key in keys.keys {
            let kid = key
                .common
                .key_id
                .as_deref()
                .filter(|kid| {
                    !kid.is_empty() && kid.len() <= 256 && !kid.chars().any(char::is_control)
                })
                .ok_or_else(|| {
                    ServiceError::invalid(
                        "Pinned JWKS keys require nonempty key IDs of at most 256 bytes.",
                    )
                })?;
            if !seen.insert(kid.to_owned()) {
                return Err(ServiceError::invalid(
                    "Pinned JWKS key IDs must be unique; remove ambiguous keys.",
                ));
            }
            if key
                .common
                .public_key_use
                .as_ref()
                .is_some_and(|usage| *usage != PublicKeyUse::Signature)
                || key
                    .common
                    .key_algorithm
                    .is_some_and(|algorithm| algorithm != KeyAlgorithm::RS256)
                || key
                    .common
                    .key_operations
                    .as_ref()
                    .is_some_and(|ops| ops.as_slice() != [KeyOperations::Verify])
            {
                continue;
            }
            let AlgorithmParameters::RSA(parameters) = &key.algorithm else {
                continue;
            };
            let modulus = URL_SAFE_NO_PAD
                .decode(&parameters.n)
                .map_err(|_| invalid_verification_key())?;
            let exponent = URL_SAFE_NO_PAD
                .decode(&parameters.e)
                .map_err(|_| invalid_verification_key())?;
            let first = modulus
                .first()
                .filter(|byte| **byte != 0)
                .ok_or_else(invalid_verification_key)?;
            let leading =
                usize::try_from(first.leading_zeros()).map_err(|_| invalid_verification_key())?;
            let bits = modulus
                .len()
                .checked_mul(8)
                .and_then(|bits| bits.checked_sub(leading))
                .ok_or_else(invalid_verification_key)?;
            let exponent_value = exponent
                .iter()
                .try_fold(0u64, |value, byte| {
                    value.checked_mul(256)?.checked_add(u64::from(*byte))
                })
                .ok_or_else(invalid_verification_key)?;
            if !(2048..=8192).contains(&bits)
                || modulus.last().is_none_or(|byte| byte % 2 == 0)
                || exponent.first().is_none_or(|byte| *byte == 0)
                || exponent.len() > 4
                || exponent_value < 3
                || exponent_value % 2 == 0
            {
                return Err(invalid_verification_key());
            }
            verification_keys.insert(
                kid.to_owned(),
                DecodingKey::from_rsa_raw_components(&modulus, &exponent),
            );
        }
        if verification_keys.is_empty() {
            return Err(ServiceError::invalid("JWKS must contain an RS256 RSA verification key; use must allow signatures and key_ops, when present, must be [verify]."));
        }
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[&config.issuer]);
        validation.set_audience(&[config.resource()]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub", "iat"]);
        validation.validate_nbf = true;
        validation.leeway = 0;
        Ok(Self {
            config,
            keys: Arc::new(verification_keys),
            validation: Arc::new(validation),
        })
    }

    pub fn authenticate(&self, token: &str) -> Result<Identity> {
        if token.len() > 16384 {
            return Err(unauthorized());
        }
        let header = decode_header(token).map_err(|_| unauthorized())?;
        if header.alg != Algorithm::RS256
            || header.crit.is_some()
            || header.enc.is_some()
            || header.zip.is_some()
            || header.extras.inner().contains_key("b64")
        {
            return Err(unauthorized());
        }
        let kid = header.kid.as_deref().ok_or_else(unauthorized)?;
        let key = self.keys.get(kid).ok_or_else(unauthorized)?;
        let claims = decode::<Claims>(token, key, &self.validation)
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

fn invalid_verification_key() -> ServiceError {
    ServiceError::invalid("Invalid RSA verification key: require a canonical 2048..8192-bit public modulus and an odd public exponent of 3..4294967295.")
}

pub fn unauthorized() -> ServiceError {
    ServiceError::new(
        "unauthorized",
        "A valid Trust access token is required.",
        "Authenticate with the configured authorization server for the Trust resource.",
    )
}
