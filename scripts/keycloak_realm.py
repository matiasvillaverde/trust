#!/usr/bin/env python3
"""Generate the reviewed Trust realm; no user credentials or private keys in output."""
import argparse
import json
from pathlib import Path
from urllib.parse import urlparse

OWNER_ACR = "urn:trust:owner:mfa"
OWNER_FLOW = "trust-owner-webauthn"
ENROLL_FLOW = "trust-enrollment"
SCOPES = ["trust:read", "trust:preview", "trust:trade", "trust:reconcile", "trust:journal"]
OWNER_SCOPES = SCOPES + ["trust:policy", "trust:admin", "trust:manual"]


def execution(authenticator, priority, config=None):
    value = {"authenticator": authenticator, "requirement": "REQUIRED", "priority": priority,
             "authenticatorFlow": False, "userSetupAllowed": False}
    if config:
        value["authenticatorConfig"] = config
    return value


def subflow(alias, priority):
    return {"flowAlias": alias, "requirement": "CONDITIONAL", "priority": priority,
            "authenticatorFlow": True, "userSetupAllowed": False}


def flow(alias, executions, top=False):
    return {"id": alias, "alias": alias, "providerId": "basic-flow", "topLevel": top,
            "builtIn": False, "authenticationExecutions": executions}


def realm(identity_url, resource, redirects, agent_certificate=None, *, local_test=False):
    """Local HTTP is available only to the isolated acceptance harness."""
    urls = [identity_url, resource, *redirects]
    for url in urls:
        parsed = urlparse(url)
        if not parsed.hostname or parsed.fragment or parsed.query or (parsed.scheme != "https" and not (local_test and parsed.scheme == "http"
                and parsed.hostname in {"localhost", "127.0.0.1"})) or parsed.username or parsed.password or "*" in url:
            raise ValueError("Use explicit HTTPS URLs without credentials or wildcard redirects")
    if urlparse(identity_url).path not in ("", "/") or urlparse(resource).path != "/mcp" or not redirects:
        raise ValueError("Use an identity origin, an exact /mcp resource and at least one callback")
    host = urlparse(identity_url).hostname
    audience = {"name": "trust-resource", "protocol": "openid-connect",
                "protocolMapper": "oidc-audience-mapper",
                "config": {"included.custom.audience": resource, "access.token.claim": "true",
                           "id.token.claim": "false", "introspection.token.claim": "true"}}
    owner = {"clientId": "trust-owner", "enabled": True, "protocol": "openid-connect",
             "publicClient": True, "standardFlowEnabled": True,
             "directAccessGrantsEnabled": False, "implicitFlowEnabled": False,
             "serviceAccountsEnabled": False, "fullScopeAllowed": False,
             "redirectUris": redirects, "webOrigins": [],
             "defaultClientScopes": ["acr", "basic", *OWNER_SCOPES],
             "optionalClientScopes": [], "protocolMappers": [audience],
             "authenticationFlowBindingOverrides": {"browser": OWNER_FLOW},
             "attributes": {"pkce.code.challenge.method": "S256", "minimum.acr.value": OWNER_ACR,
                            "default.acr.values": OWNER_ACR, "access.token.lifespan": "300",
                            "client.session.idle.timeout": "300", "client.session.max.lifespan": "300",
                            "use.refresh.tokens": "false", "standard.token.exchange.enabled": "false"}}
    agent = {"clientId": "trust-agent", "enabled": agent_certificate is not None,
             "protocol": "openid-connect", "publicClient": False,
             "clientAuthenticatorType": "client-jwt", "standardFlowEnabled": False,
             "directAccessGrantsEnabled": False, "implicitFlowEnabled": False,
             "serviceAccountsEnabled": True, "fullScopeAllowed": False,
             "redirectUris": [], "defaultClientScopes": ["basic", *SCOPES],
             "optionalClientScopes": [], "protocolMappers": [audience],
             "attributes": {"jwt.credential.certificate": agent_certificate or "",
                            "use.jwks.url": "false", "token.endpoint.auth.signing.alg": "RS256",
                            "access.token.lifespan": "300", "use.refresh.tokens": "false",
                            "standard.token.exchange.enabled": "false"}}
    enrollment = {"clientId": "trust-enrollment", "enabled": False, "protocol": "openid-connect",
                  "publicClient": True, "standardFlowEnabled": True,
                  "directAccessGrantsEnabled": False, "implicitFlowEnabled": False,
                  "serviceAccountsEnabled": False, "fullScopeAllowed": False,
                  "redirectUris": redirects, "defaultClientScopes": ["basic"],
                  "optionalClientScopes": [], "protocolMappers": [],
                  "attributes": {"pkce.code.challenge.method": "S256", "use.refresh.tokens": "false"},
                  "authenticationFlowBindingOverrides": {"browser": ENROLL_FLOW}}
    return {
        "realm": "trust", "enabled": True, "sslRequired": "none" if local_test else "all",
        "registrationAllowed": False, "resetPasswordAllowed": False, "rememberMe": False,
        "loginWithEmailAllowed": False, "duplicateEmailsAllowed": False, "editUsernameAllowed": False,
        "bruteForceProtected": True, "permanentLockout": False, "failureFactor": 5,
        "waitIncrementSeconds": 60, "maxFailureWaitSeconds": 900,
        "accessTokenLifespan": 300, "ssoSessionIdleTimeout": 300, "ssoSessionMaxLifespan": 300,
        "revokeRefreshToken": True, "refreshTokenMaxReuse": 0,
        "defaultSignatureAlgorithm": "RS256", "browserFlow": OWNER_FLOW,
        "attributes": {"acr.loa.map": json.dumps({OWNER_ACR: 2})},
        "webAuthnPolicyPasswordlessRpEntityName": "Trust owner authorization",
        "webAuthnPolicyPasswordlessRpId": host,
        "webAuthnPolicyPasswordlessSignatureAlgorithms": ["ES256", "RS256"],
        "webAuthnPolicyPasswordlessUserVerificationRequirement": "required",
        "webAuthnPolicyPasswordlessRequireResidentKey": "Yes",
        "webAuthnPolicyPasswordlessAuthenticatorAttachment": "cross-platform",
        "webAuthnPolicyPasswordlessAttestationConveyancePreference": "none",
        "webAuthnPolicyPasswordlessAvoidSameAuthenticatorRegister": True,
        "webAuthnPolicyPasswordlessCreateTimeout": 60,
        "eventsEnabled": True, "eventsListeners": ["jboss-logging"], "eventsExpiration": 2592000,
        "adminEventsEnabled": True, "adminEventsDetailsEnabled": False,
        "clients": [owner, agent, enrollment],
        "clientScopes": [{"name": scope, "protocol": "openid-connect",
                          "attributes": {"include.in.token.scope": "true", "display.on.consent.screen": "true"},
                          "protocolMappers": []} for scope in OWNER_SCOPES] + [
            {"name": "basic", "protocol": "openid-connect", "attributes": {"include.in.token.scope": "false"},
             "protocolMappers": [
                 {"name": "sub", "protocol": "openid-connect", "protocolMapper": "oidc-sub-mapper",
                  "config": {"access.token.claim": "true", "introspection.token.claim": "true"}},
                 {"name": "auth_time", "protocol": "openid-connect", "protocolMapper": "oidc-usersessionmodel-note-mapper",
                  "config": {"user.session.note": "AUTH_TIME", "claim.name": "auth_time", "jsonType.label": "long",
                             "access.token.claim": "true", "id.token.claim": "true"}},
             ]},
            {"name": "acr", "protocol": "openid-connect", "attributes": {"include.in.token.scope": "false"},
             "protocolMappers": [{"name": "acr", "protocol": "openid-connect", "protocolMapper": "oidc-acr-mapper",
                                  "config": {"access.token.claim": "true", "id.token.claim": "true"}}]},
        ],
        "authenticationFlows": [
            flow(OWNER_FLOW, [execution("auth-username-form", 10), subflow("trust-owner-level", 20),
                              subflow("trust-deny-without-key", 30)], True),
            flow("trust-owner-level", [execution("conditional-level-of-authentication", 10, "trust-level-2"),
                                        execution("webauthn-authenticator-passwordless", 20)]),
            flow("trust-deny-without-key", [execution("conditional-credential", 10, "trust-key-not-used"),
                                             execution("deny-access-authenticator", 20)]),
            flow(ENROLL_FLOW, [execution("auth-username-password-form", 10)], True),
        ],
        "authenticatorConfig": [
            {"alias": "trust-level-2", "config": {"loa-condition-level": "2", "loa-max-age": "0"}},
            {"alias": "trust-key-not-used", "config": {"credentials": "webauthn-passwordless", "included": "false"}},
        ],
        "requiredActions": [
            {"alias": "webauthn-register-passwordless", "name": "Register owner security key",
             "providerId": "webauthn-register-passwordless", "enabled": True,
             "defaultAction": False, "priority": 10, "config": {}},
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--identity-url", required=True)
    parser.add_argument("--resource", required=True, help="Trust public URL followed by /mcp")
    parser.add_argument("--redirect", action="append", required=True, help="Exact registered client callback")
    parser.add_argument("--agent-certificate", type=Path, help="Public X.509 PEM certificate, never a private key")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    certificate = args.agent_certificate.read_text() if args.agent_certificate else None
    if certificate is not None:
        if "PRIVATE KEY" in certificate or "BEGIN CERTIFICATE" not in certificate:
            parser.error("--agent-certificate must contain a public certificate")
        certificate = "".join(line for line in certificate.splitlines() if not line.startswith("---"))
    document = realm(args.identity_url, args.resource, args.redirect, certificate)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(document, indent=2) + "\n")


if __name__ == "__main__":
    main()
