# Self-hosted owner and agent authentication

Use **Keycloak 26.8.1** as Trust's reference self-hosted provider. This is a choice for Trust's requirements, not a claim that any provider is intrinsically the most secure. Its enforced WebAuthn user verification, authentication levels, client policies and signed client assertions fit our separation of human policy authority from agent trading. The repository includes a pinned container build, realm generator and real browser/MCP acceptance harness.

| Provider | Relevant capability | Decision for Trust |
| --- | --- | --- |
| [Keycloak](https://www.keycloak.org/docs/26.8.1/server_admin/) | WebAuthn/passkeys, required user verification, minimum client ACR, signed JWT client authentication | Reference implementation, exercised end to end here |
| [Authentik](https://docs.goauthentik.io/add-secure-apps/flows-stages/stages/authenticator_webauthn/) | WebAuthn stages, verification and authenticator restrictions | Credible alternative; its exact claims and recovery paths would need their own acceptance tests |
| [ZITADEL](https://zitadel.com/docs/concepts/features/selfservice) | Self-hosted identity with passkey authentication | Credible alternative; no equivalent Trust integration evidence in this repository |

## Authority boundary

The `trust-owner` client uses authorization code flow with **PKCE S256**, exact registered redirects, mandatory WebAuthn user verification and minimum ACR `urn:trust:owner:mfa`. The ACR comes from the executed authentication level. `auth_time` comes from the original authenticated session. Neither claim is a hardcoded mapper. Password grants, implicit flow, service accounts, refresh tokens and token exchange are disabled for this client. Every privileged login requires a fresh key assertion; a prior session cannot skip it.

The owner flow denies users without an enrolled credential; it cannot silently turn a login into key registration. A separate, disabled-by-default `trust-enrollment` client has no Trust audience or scopes. Enable it only for supervised onboarding, require an operator-provisioned one-time password and registration action, then disable it again and remove the bootstrap password. Register two independent keys before relying on this account. Keep the spare in a separate secure location.

Each agent needs its own Keycloak client, signing key and immutable service-account subject. The generated `trust-agent` is a starting example, disabled until a **public X.509 certificate** is supplied. It authenticates using `private_key_jwt` (RS256), has no browser/password flow and receives only trading/read/reconciliation/journal scopes. Private keys belong in the agent host's secret store; they never belong in prompts, MCP arguments or this repository. Give short-lived assertions a unique `jti` and the token endpoint as audience.

Trust independently pins issuer/public signing keys, validates audience and lifetime, and intersects token scopes with operator-owned subject/account grants. Exactly one subject is the owner. An agent cannot gain owner authority by requesting scopes, changing tool arguments, or possessing a token from the enrollment client. The default owner authentication window is 300 seconds; refreshing or replaying a request cannot extend it. Owner credentials stay in a separate human-controlled MCP session.

## YubiKeys and passkeys

The default realm requests a discoverable FIDO2 credential, cross-platform attachment, ES256/RS256 and **required user verification**. A compatible YubiKey uses a PIN or biometric verification in addition to possession; touch alone is insufficient. Keycloak documents YubiKey 5 NFC support in its [WebAuthn compatibility guidance](https://www.keycloak.org/docs/26.8.1/server_admin/).

Attachment preference and an authenticator name do **not** prove hardware custody. The supplied profile uses `none` attestation for interoperability. If product policy requires non-exportable hardware keys, deploy verified attestation trust and an approved authenticator/AAGUID policy, then test actual devices. Do not claim that a synced passkey or the virtual authenticator used in CI satisfies a hardware-only policy. Platform/synced passkeys require a deliberately reviewed change to the attachment policy.

Recover a lost key through the spare. Losing both requires a documented operator recovery: halt trading, revoke the owner's sessions and affected credentials, use private administration access with independent evidence of the owner, enroll replacement keys, and retain the audit record. Do not enable public password reset or a password-only owner fallback to recover availability. Keycloak administration, database backups and signing keys are part of the product's trusted computing base.

## Prepare deployment

The patched reference images pass the 2026-10-28 High/Critical dependency scans without suppression and the real PostgreSQL/browser/MCP acceptance tests. See [the remediation and maintenance requirements](security.md#current-image-findings-2026-10-28). Production ingress, physical keys, key custody and recovery remain deployment acceptance requirements.

The reference stack is [deploy/keycloak/compose.yaml](../deploy/keycloak/compose.yaml). It builds Trust's patched Keycloak and PostgreSQL images from pinned upstream inputs. The database has no published port. TLS is mandatory, and the identity listener binds to loopback for an operator-managed ingress. Health/metrics remain private. Admin-event details are disabled and access logs omit queries and headers that could contain codes or tokens.

1. Follow the [storage and host encryption requirements](security.md). Keycloak PostgreSQL, signing keys, its dedicated transaction-recovery volume and gateway state need encrypted host volumes independently of Trust SQLCipher. Create `deploy/keycloak/local/` with mode `0700` on encrypted storage; materialize secret configuration/TLS keys on private RAM mounts for deployment. Provision the database password, matching `keycloak.conf`, TLS certificate and key through the deployment secret manager. Use [keycloak.conf.example](../deploy/keycloak/keycloak.conf.example); set ownership so the container's UID 1000 can read its configuration/key. The database password file must also be readable by the PostgreSQL image's `postgres` UID after its entrypoint drops root privileges (currently UID 70); keep each private file mode `0600`. Never commit these files.
2. Generate the public realm configuration, substituting real HTTPS origins and exact MCP-client callback URLs:

   ```bash
   python3 scripts/keycloak_realm.py \
     --identity-url https://identity.example.com \
     --resource https://trust.example.com/mcp \
     --redirect https://owner-client.example.com/oauth/callback \
     --agent-certificate /secure/public-agent-certificate.pem \
     --output deploy/keycloak/local/trust-realm.json
   ```

3. Build, scan and run the isolated compatibility test below before promoting the resulting images. Set `TRUST_IDENTITY_URL` and `TRUST_IDENTITY_ADMIN_URL`, then start the accepted local images with `docker compose -f deploy/keycloak/compose.yaml up -d --no-build`. For a registry deployment, pin the accepted digests in the deployment configuration. Configure ingress to expose only the required realm/OIDC endpoints. A separate admin hostname **does not itself block admin paths** on the public ingress. Deny `/admin`, `/realms/master` and management endpoints publicly, require private administration access, and test these denials.
4. Follow Keycloak's [bootstrap/recovery procedure](https://www.keycloak.org/server/bootstrap-admin-recovery) through the private administration path. Remove temporary administration accounts after establishing independently protected operator administration. Import a realm only during initial provisioning; later edits need a reviewed migration, not an assumption that `--import-realm` overwrites existing configuration.
5. Supervise owner enrollment, disable the enrollment client, and copy the owner's immutable subject into Trust's grant. Create a distinct client/subject for each agent with only its permitted Trust account IDs. Set each agent's mandate through the human owner session.
6. Obtain public JWKS from the verified issuer and mount them at Trust's configured `jwks_file`. Trust intentionally uses pinned keys rather than fetching arbitrary JWT URLs. For rotation, stage both old and new public keys, restart Trust, rotate the issuer, wait for old tokens to expire, then remove retired keys and restart. Test both overlap and retirement before production rollout.
7. Retain identity and Trust audit logs outside the writable application host. Back up Keycloak PostgreSQL/signing keys and Trust's stopped financial/control databases. Exercise restore and key revocation. Alert on login failures, privileged mutations, unresolved operations and durable halts.

Keep dynamic client registration disabled unless a specific MCP client's requirements have been reviewed. Clients must support a pre-registered client ID and exact callback; a client that insists on unrestricted dynamic registration is not yet approved. Neither this stack nor the local harness provisions a public hostname or production certificates.

## Run the isolated compatibility test

Requirements: Docker, Node, `uv`, OpenSSL, Trivy, and the Rust toolchain. This starts disposable local Keycloak/PostgreSQL containers and a Trust process, uses ephemeral credentials, and never connects to a broker. `make oidc-acceptance` builds both images and runs the functional checks; for a release rebuild and scan of the exact artifacts, use:

```bash
make native-sqlcipher
cargo build --locked -p trust-service
docker build --pull --no-cache -t trust-keycloak:26.8.1 deploy/keycloak
docker build --pull --no-cache -t trust-postgres:18.6 deploy/postgres
trivy image --scanners vuln --severity HIGH,CRITICAL --ignore-unfixed=false --exit-code 1 trust-keycloak:26.8.1
trivy image --scanners vuln --severity HIGH,CRITICAL --ignore-unfixed=false --exit-code 1 trust-postgres:18.6
npm ci --prefix service/tests/oidc
service/tests/oidc/node_modules/.bin/playwright install chromium
uv sync --locked --project service/tests/oidc
python3 service/tests/oidc/run.py
```

The harness checks the actual Guava bytes/version, Java/Go versions and Java trust store, then exercises real browser registration/login with a virtual CTAP2 authenticator, genuine signed tokens, official TypeScript/Python MCP clients, restricted discovery, and audited owner/agent calls. Negative cases include no enrolled key, absent user verification, missing PKCE, password/refresh grants, reused client assertions and expired owner authentication. PostgreSQL checks cover nonroot initialization, rollback, wrong-password rejection and persisted data/signing keys/passkeys after a cold restart. Fresh owner and agent grants must work after restart. It deletes its isolated containers, networks, volumes and credential directory on exit. Redacted evidence is written to `output/playwright/oidc/evidence.json`; a rerun removes an earlier passing record before testing. Tokens, passwords, private keys and authorization codes are not evidence artifacts.

This is an executable integration test, not production certification. Physical keys, browser/device combinations, the actual chosen end-user MCP clients, production TLS/ingress, operator recovery and rotation remain deployment acceptance requirements. See [verification.md](reliability/verification.md) for the current executed record.
