#!/usr/bin/env python3
"""Isolated real Keycloak + Chromium + MCP SDK test. Never connects to a broker."""
import importlib.util
import hashlib
import io
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import tempfile
import time
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location("keycloak_realm", ROOT / "scripts/keycloak_realm.py")
REALM = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REALM)
IMAGE = "trust-keycloak:26.8.1"
DATABASE_IMAGE = "trust-postgres:18.6"
OWNER = "7f5f0327-c033-48cc-a608-3f07c49c6381"
AGENT = "7f5f0327-c033-48cc-a608-3f07c49c6382"


def port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def ready(url, seconds=90):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            with urllib.request.urlopen(url, timeout=2) as response:
                return json.load(response)
        except (OSError, ValueError):
            time.sleep(0.25)
    raise RuntimeError("Local test process did not become ready")


def docker(*args, **kwargs):
    return subprocess.run(["docker", *args], check=True, capture_output=True,
                          timeout=90, **kwargs).stdout


def image_contracts():
    """Inspect the shipped bytes; a clean scanner result alone is insufficient."""
    jar = docker("run", "--rm", "--network=none", "--entrypoint=cat", IMAGE,
                 "/opt/keycloak/lib/lib/main/com.google.guava.guava-33.6.0-jre.jar")
    checksum = hashlib.sha256(jar).hexdigest()
    if checksum != "b530942257fb935f8b2cfaa5f8eb5bd59c555fd8e8d01b8ce98912e077ea606c":
        raise RuntimeError("Unexpected Guava bytes in production image")
    with zipfile.ZipFile(io.BytesIO(jar)) as archive:
        properties = archive.read("META-INF/maven/com.google.guava/guava/pom.properties")
        if b"version=33.7.2-jre" not in properties:
            raise RuntimeError("Guava Maven metadata does not identify the fixed release")
    listing = docker("run", "--rm", "--network=none", "--entrypoint=ls", IMAGE,
                     "/opt/keycloak/lib/lib/main").decode().splitlines()
    if [name for name in listing if name.startswith("com.google.guava.guava-")] != [
            "com.google.guava.guava-33.6.0-jre.jar"]:
        raise RuntimeError("Unexpected duplicate Guava dependency")
    # Oracle's mitigation explicitly removes all four roots. Wolfi shares its
    # Java keystore with java-cacerts, so verify contents independently of aliases.
    certificates = docker("run", "--rm", "--network=none",
                          "--entrypoint=/usr/lib/jvm/java-21-openjdk/bin/keytool", IMAGE,
                          "-list", "-v", "-cacerts", "-storepass", "changeit").decode()
    for name in ("emSign ECC Root CA - G3", "AAA Certificate Services",
                 "emSign Root CA - G1", "emSign Root CA - G2"):
        if "CN=" + name in certificates:
            raise RuntimeError("Java trust store still contains an explicitly distrusted root")
    java_version = docker("run", "--rm", "--network=none",
                          "--entrypoint=/usr/lib/jvm/java-21-openjdk/bin/java", IMAGE, "--version").decode().strip()
    gosu_version = docker("run", "--rm", "--network=none", "--entrypoint=gosu",
                          DATABASE_IMAGE, "--version").decode().strip()
    if "21.0.13.0.2" not in java_version or "go1.27.3" not in gosu_version:
        raise RuntimeError("Production images do not contain the accepted Java/Go versions")
    print("PASS shipped Guava bytes/metadata and Java CA trust mitigation", flush=True)
    return {"guava_sha256": checksum, "java_version": java_version, "gosu_version": gosu_version,
            "identity_image_id": json.loads(docker("image", "inspect", IMAGE))[0]["Id"],
            "database_image_id": json.loads(docker("image", "inspect", DATABASE_IMAGE))[0]["Id"]}


def database_ready(container):
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        result = subprocess.run(["docker", "exec", container, "pg_isready", "-h", "127.0.0.1", "-U", "keycloak",
                                 "-d", "keycloak"], capture_output=True, timeout=5)
        if result.returncode == 0:
            return
        time.sleep(0.25)
    raise RuntimeError("Isolated PostgreSQL did not become ready")


def database_contracts(container, network, password_file):
    # Fresh init exercises the rebuilt gosu entrypoint and its privilege drop.
    status = docker("exec", container, "cat", "/proc/1/status").decode().splitlines()
    uid = next(line for line in status if line.startswith("Uid:")).split()[1:]
    if not uid or any(value == "0" for value in uid):
        raise RuntimeError("PostgreSQL must run without root after initialization")
    version = docker("exec", container, "psql", "-U", "keycloak", "-d", "keycloak",
                     "-Atc", "SHOW server_version").decode().strip()
    if not version.startswith("18.6"):
        raise RuntimeError("Unexpected PostgreSQL runtime version")
    docker("exec", container, "psql", "-U", "keycloak", "-d", "keycloak",
           "-v", "ON_ERROR_STOP=1", "-c",
           "CREATE TABLE trust_acceptance (value integer PRIMARY KEY); "
           "INSERT INTO trust_acceptance VALUES (1);")
    docker("exec", container, "psql", "-U", "keycloak", "-d", "keycloak",
           "-v", "ON_ERROR_STOP=1", "-c", "BEGIN; "
           "INSERT INTO trust_acceptance VALUES (2); ROLLBACK;")
    rows = docker("exec", container, "psql", "-U", "keycloak", "-d", "keycloak",
                  "-Atc", "SELECT sum(value) FROM trust_acceptance").decode().strip()
    if rows != "1":
        raise RuntimeError("PostgreSQL rollback did not preserve the committed row")
    # Deliberately wrong ephemeral password: prove network authentication is on.
    password_file.write_text("database:5432:keycloak:keycloak:" + secrets.token_urlsafe(32) + "\n")
    password_file.chmod(0o600)
    result = subprocess.run(["docker", "run", "--rm", "--network", network,
                             "--entrypoint=psql", "--env", "PGPASSFILE=/run/test-password",
                             "--mount", f"type=bind,source={password_file},target=/run/test-password,readonly",
                             DATABASE_IMAGE, "-h", "database", "-U", "keycloak", "-d", "keycloak",
                             "--no-password", "-c", "SELECT 1"], capture_output=True, timeout=30)
    if result.returncode == 0 or b"password authentication failed" not in result.stderr:
        raise RuntimeError("PostgreSQL did not explicitly reject incorrect network credentials")
    print("PASS PostgreSQL non-root initialization, transaction rollback and password rejection", flush=True)


def main():
    binary = Path(os.environ.get("TRUST_TEST_BINARY", ROOT / "target/debug/trust-service")).resolve()
    if not binary.is_file():
        raise SystemExit("Build first: cargo build -p trust-service")
    artifact = ROOT / "output/playwright/oidc"
    artifact.mkdir(parents=True, exist_ok=True)
    # A failed rerun must not leave an earlier passing evidence record in place.
    (artifact / "evidence.json").unlink(missing_ok=True)
    images = image_contracts()
    container = "trust-oidc-test-" + secrets.token_hex(5)
    database = container + "-database"
    network = container + "-network"
    identity_network = container + "-identity"
    volume = container + "-data"
    transactions = container + "-transactions"
    service = None
    # All secrets and private keys are ephemeral; only redacted test evidence persists.
    with tempfile.TemporaryDirectory(prefix="trust-oidc-", dir="/dev/shm" if Path("/dev/shm").is_dir() else None) as directory:
        folder = Path(directory)
        os.chmod(folder, 0o700)
        identity = f"http://localhost:{port()}"
        origin = f"http://127.0.0.1:{port()}"
        callback = f"http://localhost:{port()}/callback"
        password = secrets.token_urlsafe(32)
        database_password = secrets.token_urlsafe(32)
        database_secret = folder / "database-password"
        database_secret.write_text(database_password)
        database_secret.chmod(0o600)
        keycloak_config = folder / "keycloak.conf"
        keycloak_config.write_text("cache=local\ndb=postgres\ndb-url-host=database\ndb-url-database=keycloak\n"
                                   "db-username=keycloak\ndb-password=" + database_password + "\n")
        keycloak_config.chmod(0o600)
        subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
                        "-subj", "/CN=trust-local-agent", "-keyout", str(folder / "agent.key"),
                        "-out", str(folder / "agent.crt")], check=True, capture_output=True)
        certificate = "".join(line for line in (folder / "agent.crt").read_text().splitlines()
                              if not line.startswith("---"))
        document = REALM.realm(identity, origin + "/mcp", [callback], certificate, local_test=True)
        for client in document["clients"]:
            if client["clientId"] == "trust-enrollment":
                client["enabled"] = True  # Test-only onboarding; no Trust audience or scopes.
        document["users"] = [
            {"id": OWNER, "username": "owner", "enabled": True, "emailVerified": True,
             "firstName": "Test", "lastName": "Owner", "email": "owner@example.invalid",
             "requiredActions": ["webauthn-register-passwordless"],
             "credentials": [{"type": "password", "value": password, "temporary": False}]},
            {"username": "no-key", "enabled": True, "emailVerified": True,
             "firstName": "Test", "lastName": "NoKey", "email": "no-key@example.invalid",
             "credentials": [{"type": "password", "value": password, "temporary": False}]},
            {"id": AGENT, "username": "service-account-trust-agent", "enabled": True,
             "serviceAccountClientId": "trust-agent"},
        ]
        realm_path = folder / "trust-realm.json"
        realm_path.write_text(json.dumps(document))
        # The parent remains 0700; the non-root container sees only this bind mount.
        os.chmod(realm_path, 0o644)
        options = {"identity": identity, "origin": origin, "callback": callback, "password": password,
                   "agentKey": str(folder / "agent.key"), "artifacts": str(artifact), "ownerSubject": OWNER,
                   "agentSubject": AGENT, "sdkCredentials": str(folder / "sdk.json"),
                   "identityContainer": container, "databaseContainer": database, "images": images,
                   "infrastructureChecks": ["shipped Guava bytes/metadata, patched Java/Go and Java CA trust mitigation",
                                            "PostgreSQL non-root initialization, transaction rollback and password rejection"]}
        (folder / "test.json").write_text(json.dumps(options))
        (folder / "test.json").chmod(0o600)
        try:
            docker("network", "create", "--internal", network)
            docker("network", "create", identity_network)
            docker("volume", "create", volume)
            docker("volume", "create", transactions)
            postgres_uid = docker("run", "--rm", "--network=none", "--entrypoint=id",
                                  DATABASE_IMAGE, "-u", "postgres").decode().strip()
            if not postgres_uid.isdecimal() or postgres_uid == "0":
                raise RuntimeError("PostgreSQL image has an invalid service UID")
            # Match container UIDs without requiring the host/CI user to be root.
            # These mounts expose only the two freshly generated test fixtures.
            for path, owner in ((database_secret, postgres_uid), (keycloak_config, "1000")):
                docker("run", "--rm", "--network=none", "--cap-drop=ALL", "--cap-add=CHOWN",
                       "--entrypoint=chown", "--mount", f"type=bind,source={path},target=/fixture",
                       DATABASE_IMAGE, owner, "/fixture")
            docker("run", "--detach", "--name", database, "--network", network,
                   "--network-alias=database", "--read-only", "--security-opt=no-new-privileges",
                   "--memory=1g", "--memory-swap=1g", "--cpus=2", "--pids-limit=128", "--ulimit=core=0",
                   "--env=POSTGRES_USER=keycloak", "--env=POSTGRES_DB=keycloak",
                   "--env=POSTGRES_PASSWORD_FILE=/run/secrets/database-password",
                   "--mount", f"type=volume,source={volume},target=/var/lib/postgresql",
                   "--mount", f"type=bind,source={database_secret},target=/run/secrets/database-password,readonly",
                   "--tmpfs=/tmp:rw,noexec,nosuid,size=16m",
                   "--tmpfs=/var/run/postgresql:rw,nosuid,size=16m", DATABASE_IMAGE)
            database_ready(database)
            database_contracts(database, network, folder / "wrong-password")
            subprocess.run(["docker", "run", "--detach", "--name", container,
                            "--network", identity_network, "--network", network,
                            "--read-only", "--init", "--ulimit=core=0",
                            "--user", "1000:0", "--cap-drop=ALL", "--security-opt=no-new-privileges",
                            "--memory=2g", "--memory-swap=2g", "--cpus=2", "--pids-limit=256",
                            "--publish", f"127.0.0.1:{url_port(identity)}:8080",
                            "--mount", f"type=bind,source={realm_path},target=/opt/keycloak/data/import/trust-realm.json,readonly",
                            "--mount", f"type=bind,source={keycloak_config},target=/opt/keycloak/conf/keycloak.conf,readonly",
                            "--mount", f"type=volume,source={transactions},target=/opt/keycloak/data/transaction-logs",
                            "--tmpfs=/tmp:rw,noexec,nosuid,size=64m,mode=1777",
                            "--tmpfs=/opt/keycloak/data/tmp:rw,nosuid,size=64m,uid=1000,gid=0",
                            IMAGE, "start", "--optimized", "--http-enabled=true", "--import-realm", "--hostname", identity,
                            "--log-level=warn"], check=True, capture_output=True)
            discovery = ready(identity + "/realms/trust/.well-known/openid-configuration")
            keys = ready(discovery["jwks_uri"])
            (folder / "jwks.json").write_text(json.dumps(keys))
            config = {"listen": "127.0.0.1:" + str(url_port(origin)), "public_url": origin,
                      "issuer": identity + "/realms/trust", "jwks_file": str(folder / "jwks.json"),
                      "database": str(folder / "financial.db"), "journal": str(folder / "control.db"),
                      "reconcile_interval_seconds": 0, "allow_live": False,
                      "owner_authentication": {"acr_values": [REALM.OWNER_ACR], "max_age_seconds": 15},
                      "grants": {OWNER: {"role": "owner", "all_accounts": True, "scopes": REALM.OWNER_SCOPES},
                                 AGENT: {"role": "agent", "all_accounts": True, "scopes": REALM.SCOPES}}}
            (folder / "config.json").write_text(json.dumps(config))
            with (artifact / "trust.log").open("w") as log:
                service = subprocess.Popen([str(binary), str(folder / "config.json")], stdout=log, stderr=log)
                ready(origin + "/healthz")
                subprocess.run(["node", str(Path(__file__).with_name("acceptance.mjs")), str(folder / "test.json")], check=True)
        finally:
            if service is not None:
                service.terminate()
                try:
                    service.wait(timeout=30)
                except subprocess.TimeoutExpired:
                    service.kill()
                    service.wait()
            # Provider logs contain no requests/tokens at this test log level.
            for name, log_name in ((container, "keycloak.log"), (database, "postgres.log")):
                logs = subprocess.run(["docker", "logs", name], capture_output=True)
                (artifact / log_name).write_bytes(logs.stdout + logs.stderr)
                subprocess.run(["docker", "rm", "--force", name], capture_output=True)
            subprocess.run(["docker", "volume", "rm", volume], capture_output=True)
            subprocess.run(["docker", "volume", "rm", transactions], capture_output=True)
            subprocess.run(["docker", "network", "rm", network], capture_output=True)
            subprocess.run(["docker", "network", "rm", identity_network], capture_output=True)


def url_port(url):
    from urllib.parse import urlparse
    return urlparse(url).port


if __name__ == "__main__":
    main()
