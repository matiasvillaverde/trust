import unittest
import uuid
from urllib.parse import urlunsplit

from keycloak_realm import OWNER_ACR, OWNER_SCOPES, realm


class RealmTests(unittest.TestCase):
    def test_only_explicit_tls_origins_resources_and_callbacks(self):
        valid = ["https://id.example.com", "https://trust.example.com/mcp", ["https://client.example.com/callback"]]
        # Generated, never-valid userinfo exercises rejection without embedding a credential.
        credential_url = urlunsplit(("https", "test-user:" + uuid.uuid4().hex + "@id.example.com", "", "", ""))
        for index, bad in [(0, "https:///missing"), (0, "http://id.example.com"),
                           (0, credential_url), (0, "https://id.example.com/realm"),
                           (1, "https://trust.example.com/"), (1, "https://trust.example.com/mcp#frag"),
                           (2, []), (2, ["https://*.example.com/callback"]),
                           (2, ["https://client.example.com/callback?secret=value"])]:
            args = valid.copy()
            args[index] = bad
            with self.assertRaises(ValueError):
                realm(*args)

    def test_separate_authority_and_supervised_enrollment(self):
        doc = realm("https://id.example.com", "https://trust.example.com/mcp", ["https://client.example.com/callback"])
        clients = {client["clientId"]: client for client in doc["clients"]}
        owner, agent, enrollment = [clients[name] for name in ("trust-owner", "trust-agent", "trust-enrollment")]
        self.assertEqual(owner["attributes"]["minimum.acr.value"], OWNER_ACR)
        self.assertEqual(owner["attributes"]["pkce.code.challenge.method"], "S256")
        self.assertFalse(owner["directAccessGrantsEnabled"] or owner["serviceAccountsEnabled"])
        self.assertEqual(doc["webAuthnPolicyPasswordlessUserVerificationRequirement"], "required")
        self.assertFalse(agent["enabled"] or agent["standardFlowEnabled"])
        self.assertEqual(agent["clientAuthenticatorType"], "client-jwt")
        self.assertFalse(set(agent["defaultClientScopes"]) & {"trust:policy", "trust:admin", "trust:manual"})
        self.assertFalse(enrollment["enabled"])
        self.assertFalse(set(enrollment["defaultClientScopes"]) & set(OWNER_SCOPES))
        self.assertEqual(enrollment["protocolMappers"], [])
        self.assertFalse(doc["resetPasswordAllowed"] or doc["registrationAllowed"])


if __name__ == "__main__":
    unittest.main()
