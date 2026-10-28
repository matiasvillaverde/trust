# Encryption and server security

Trust protects production financial storage, its operation/audit journal, and service-exported financial backups with **SQLCipher 4.19.0**. Production startup refuses missing encryption configuration, unsafe key files, reused keys, and disk-backed runtime secret mounts. This is implemented protection for stored data; it does not make a compromised running service or host safe to trade.

## Storage and trust boundaries

| Material | Protection and custody |
| --- | --- |
| Financial database | SQLCipher with its own random 256-bit key; private directory/files. |
| Control database: audit, idempotency, mandates, halts | SQLCipher with an independent random 256-bit key. It must be recovered with the matching financial state. |
| MCP financial backup | A SQLCipher database encrypted with a third independent key. No plaintext JSON export from the service. |
| Broker credentials, database keys, optional calendar/distribution secrets | Owner/root-owned regular files, mode `0600`, supplied on Linux tmpfs/ramfs for production. Never stored in tool arguments, configuration JSON, Git or image layers. |
| Keycloak PostgreSQL/signing keys, gateway session state, logs, snapshots, swap | Operator-provisioned encrypted host volumes and backup storage. Trust's database encryption does not cover these. |
| Runtime secrets | Restricted process/container access, bounded reads, zeroized key buffers, disabled core dumps. The process necessarily can decrypt its own data and authorize broker requests. |

SQLCipher 4 uses AES-256-CBC encryption and per-page HMAC-SHA512 authentication. Database pages in WAL/rollback journals are encrypted too; shared-memory files contain coordination metadata. Trust forces temporary SQL storage into memory. File names, sizes and access patterns remain visible. Page authentication detects corruption, but does not prove that an attacker has not replaced a complete database with an older valid copy. Retain audit-head evidence and backups outside the application host. See the upstream [design](https://www.zetetic.net/sqlcipher/design/) and [API contract](https://www.zetetic.net/sqlcipher/sqlcipher-api/).

The native source is pinned by version and SHA-256 in `scripts/build_sqlcipher.py`. The 4.19 release includes fixes absent from the Rust dependency's older bundled C; see the [release advisory](https://www.zetetic.net/blog/2026/09/08/sqlcipher-4.19.0-release/). The server verifies a supported patched SQLCipher version before applying a key, and reads the schema before accepting it. An ordinary SQLite library cannot silently ignore encryption. SQLCipher 5 prereleases are not accepted. This uses an established database encryption implementation, not custom cryptography.

## Provision keys and storage

Use a dedicated Linux service host or VM. Enable LUKS2 or an equivalent encrypted block volume for application data, Keycloak, gateway state and local log storage. Verify provider snapshots and remote backups have encryption and access controls too. Disable host swap/hibernation or encrypt them appropriately; tmpfs alone does not prevent swapping. The Compose profiles disable container swap and core dumps, but they do not configure the host's disk encryption.

Keep long-term keys outside the data volume and outside the application host's backups. For a self-hosted deployment, [OpenBao](https://openbao.org/docs/internals/security/) is a suitable separate secret store: restrict each workload to its own paths and deliver files through an authenticated provisioning agent. Unseal/recovery custody needs independent operators or hardware-backed controls. OpenBao is a deployment recommendation, not a component installed or configured by this repository. Its security model also excludes a compromised host/root reading runtime memory.

Generate three keys once for a new deployment. The operator utility writes a new `0600` file exclusively and never prints the key. It is intentionally unavailable as an MCP tool. For example, after building the image:

```sh
docker build -t trust-service:local .
# /run must be backed by tmpfs on this Linux host.
sudo install -d -o 10001 -g 10001 -m 0700 /run/trust-secrets
for kind in financial journal backup; do
  docker run --rm --network none --read-only --cap-drop ALL \
    --security-opt no-new-privileges:true --ulimit core=0:0 \
    --user 10001:10001 \
    --mount type=bind,src=/run/trust-secrets,dst=/run/trust-secrets \
    --entrypoint /usr/local/bin/trust-storage trust-service:local \
    keygen "/run/trust-secrets/$kind-key.hex"
done
```

Securely escrow these keys in the external secret store before storing financial data. Test recovery without copying keys into terminals, tickets or logs. After reboot, materialize the **same** keys from custody; do not put key generation in a startup script. Losing a key means losing its encrypted data. Retain historical backup keys for retained backup artifacts.

Use the `storage_encryption` block in `deploy/config.example.json`, whose three paths point into `/run/trust-secrets`. Set `TRUST_SECRETS_DIR=/run/trust-secrets` for Compose. The host directory must be private and readable by container UID 10001. Map ownership appropriately if using user namespaces. Key contents must be independent 64-character hex values; key and database paths must be absolute, without parent traversal, and keys must reside outside both database directories.

Public configuration and JWKS contain no private signing or broker keys. They must be owned by root or the service UID, not writable by group/others, and not symlinked. For the bind-mounted example:

```sh
sudo chown root:root deploy/config.local.json deploy/jwks.json
sudo chmod 0644 deploy/config.local.json deploy/jwks.json
export TRUST_SECRETS_DIR=/run/trust-secrets
docker compose -f deploy/compose.yaml up -d
```

Data directories must be owned by the service, mode `0700`; database, backup and secret files are `0600`. Existing public files/directories are rejected rather than silently chmodded. Secret reads reject symlinks, multiple hard links, FIFOs/devices, untrusted writable parents, oversized contents and incorrect ownership. Production mounts on persistent filesystems are rejected even if read-only. A Compose bind mount is transport for secret files, not encrypted secret custody. The sample Docker context excludes deployment secret directories and key files.

Native service development requires Linux, Python 3.12+, a C compiler, make, pkg-config and OpenSSL headers:

```sh
make native-sqlcipher
cargo build --locked -p trust-service
```

The Linux image and CI use the same verified native build. Local HTTP development bound entirely to loopback with live trading disabled may omit `storage_encryption`; those development databases are plaintext. Service backup tools still require encryption. The standalone legacy CLI's JSON backup and plaintext SQLite workflows are unchanged and must never access production service storage.

## Migrate plaintext databases and rotate keys

Stop Trust and other writers first. Preserve the original financial/control pair, configuration, keys, and any WAL files as one protected recovery set. Do not copy only a main database while a writer is running. Migration/rotation is an offline operator action, never an agent tool.

The `trust-storage` binary supports:

```text
trust-storage keygen NEW_KEY_FILE
trust-storage encrypt-copy SOURCE_DB NEW_DEST_DB DEST_KEY_FILE [SOURCE_KEY_FILE]
```

Run the utility as the service identity (UID 10001 in the image), with the stopped data directories and keys mounted at the specified paths. For a native build, the binary is `target/debug/trust-storage` or `target/release/trust-storage`. For a stopped plaintext deployment, prepare private destination directories and run both copies:

```sh
trust-storage encrypt-copy /secure/old/financial.db /secure/new/financial.db /run/trust-secrets/financial-key.hex
trust-storage encrypt-copy /secure/old/control.db /secure/new/control.db /run/trust-secrets/journal-key.hex
```

The tool takes the same exclusive file lock as the service, refuses existing destinations, validates integrity/foreign keys, and uses SQLCipher export into a new encrypted file. It preserves schema, rows, indexes, triggers, sequences, `user_version` and `application_id`, then reopens and verifies the encrypted output and syncs it to disk. It does not perform an in-place conversion or delete the source. Source access requires write permission because SQLite's ATTACH inherits the connection mode; the copy executes no source data mutation. Start from a cleanly stopped, healthy database rather than an unresolved hot journal.

For rotation, pass the old key as the final argument and a newly escrowed key as the destination key. Copy both databases, verify the audit chain, balances, positions, unresolved operations and broker reconciliation, then switch the configuration to the new paths/keys. Retain the previous recovery pair until the new deployment is accepted. Backup-key rotation affects newly exported artifacts; it does not re-encrypt historical artifacts automatically.

The utility leaves old plaintext files, snapshots and filesystem remnants untouched. Deleting a file is not proof of erasure, especially on SSDs or copy-on-write filesystems. Retire plaintext volumes and snapshots according to the storage provider's secure-erasure/cryptographic-erasure procedure.

Online `backup_export` returns an opaque artifact ID for an encrypted financial database. `backup_restore` needs that artifact's backup key and an empty financial database. This is not a full disaster-recovery bundle: retain the matching control journal separately to preserve audit, halts and idempotency. Restoring a ledger never reverses broker orders, fills or transfers. Reconcile before permitting new exposure.

## Identity, network and runtime controls

Use the [Keycloak owner-security-key profile](authentication.md), a spare physical authenticator, and tested operator recovery. Passkeys/YubiKeys protect human sign-in. They do not automatically encrypt disks, prevent a stolen bearer token from being used, or stop a compromised trading process from using an already-loaded broker credential. Agents receive Trust scopes/mandates only; do not deliver owner sessions or broker keys to agent containers.

Run the service as the dedicated unprivileged identity with a read-only root, dropped capabilities, no-new-privileges, process/memory limits and private writable data. Do not mount the Docker socket, host root, or agent workspaces. Core dumps and same-user Linux dumpability are disabled before secrets are loaded. SQLCipher memory security and zeroizing Rust secret buffers reduce remnants; third-party clients and general application values are not guaranteed to erase every runtime copy.

Expose only the authenticated HTTPS MCP/API route through ingress. Keep Keycloak administration, PostgreSQL, broker gateway and metrics private; restrict egress to the required issuer/broker/monitoring endpoints at the deployment network layer. The Compose example does not implement a complete host firewall or outbound allowlist. Verify broker-side permissions and disable transfers/withdrawals wherever the broker permits. Gateway session access can be equivalent to trading authority.

Ship redacted operational logs and audit-head evidence to independently controlled storage. Alert on failed owner authentication, credential use changes, unresolved outcomes, reconciliation failures and persistent halts. Exercise broker credential revocation, owner-key loss, encrypted restore, disk-full behavior and restart before live use. Keep the native SQLCipher pin, Rust advisory scans and container/OpenSSL updates current.

## Executable evidence and remaining acceptance

Storage tests cover unsafe files/paths, independent keys, RAM mounts, wrong keys, plaintext rejection, corruption, exclusive-copy locking, key rotation and encrypted restore. The real-process E2E test covers a simulated broker lifecycle, encrypted financial/control/WAL files, restart/idempotency, backup restore and failure to start with the wrong key. The container smoke covers production key mounts, identity/permissions, owner calls, encrypted persistence and restart. The release latency gate uses encrypted storage.

See [the verification record](reliability/verification.md) for executed results. These tests do not establish that a production host has encrypted disks, a functioning secret manager, secure physical authenticators, a tested disaster-recovery site, or real IBKR acceptance. Those are deployment acceptance requirements; no production infrastructure or live broker credentials were changed by this implementation.

## Current image findings (2026-10-28)

The final rebuilt **Trust, Keycloak and PostgreSQL images each passed Trivy 0.76.1 with zero High/Critical package findings**, including unfixed findings and without suppressions. The earlier failures remain in the historical verification record. This remediation addresses the following dependency findings:

| Image | Original findings | Remediation |
| --- | --- | --- |
| Keycloak 26.8.1 | 3 High | Keep the pinned upstream application distribution, use Wolfi OpenJDK **21.0.13.0.2-r0**, replace Guava with verified **33.7.2-jre**, and re-augment Keycloak. This fixes the reported Java CPU/CA and Guava findings. |
| PostgreSQL 18.6 Alpine | 27 High, 1 Critical | Update Alpine runtime packages, including brotli and curl, and rebuild **gosu 1.19** from verified source with **Go 1.27.3**. Preserve PostgreSQL's official entrypoint and data layout. |

Source builds are in [deploy/keycloak/Dockerfile](../deploy/keycloak/Dockerfile) and [deploy/postgres/Dockerfile](../deploy/postgres/Dockerfile). Base images and downloaded sources/JARs have digest or SHA-256 pins. Wolfi/Alpine packages come from their signed repositories. Quarkus embeds the Guava classpath filename, so the replacement keeps that filename while its actual bytes and Maven metadata identify 33.7.2-jre. The acceptance harness checks the full JAR hash, embedded version and absence of duplicate Guava JARs. This follows Keycloak's documented [dependency JAR replacement/rebuild workflow](https://github.com/keycloak/keycloak/blob/26.8.1/quarkus/README.md); the [Guava advisory](https://github.com/google/guava/security/advisories/GHSA-xxph-c9ww-hj94) identifies the fixed release.

The newer **CVE-2026-114820** also affects Java certificate trust. A clean package scan did not catch that Wolfi's shared Java keystore still contained one of the roots listed in the advisory. The image applies the upstream [root-removal mitigation](https://ops.java/emudhra/) and acceptance inspects the resulting keystore independently of package versions and aliases. All four named roots must be absent. This is a documented mitigation on 21.0.13.0.2, not a claim that the JVM is 21.0.13.0.3. Do not reintroduce those roots through a custom trust store. Track the [security alert](https://www.oracle.com/security-alerts/alert-cve-2026-114820.html) when updating Java.

The real browser/SDK harness now uses these production-mode images with **PostgreSQL**, a read-only Keycloak root filesystem and a persistent recovery-log volume. It verifies initialization/privilege dropping, password rejection, transaction rollback, passkey/agent authentication and new owner/agent grants after restarting both services. Keep both identity volumes on encrypted host storage. The test uses loopback HTTP and a virtual authenticator; production TLS, physical keys and operational recovery still require deployment acceptance.

The GitHub Actions container-security gate scans **all three images**; release validation also runs the actual OIDC/PostgreSQL/MCP acceptance harness. Local scan reports and build logs are under `.local/image-remediation/`; redacted authentication evidence, including image IDs and runtime versions, is in `output/playwright/oidc/evidence.json`. Rust advisory checks run separately because the image scanner does not identify Rust dependencies or statically linked native SQLCipher.

Rebuild with `--pull --no-cache` for release updates, scan the exact resulting artifacts, run the direct acceptance harness against those images, and promote the tested images by immutable digest. The [authentication runbook](authentication.md#run-the-isolated-compatibility-test) gives the commands. Do not rebuild a different artifact during production rollout. These are Trust-maintained dependency overlays; remove them only when an upstream image contains equivalent fixes and passes the same checks. New advisories must fail the gate rather than be hidden with `ignore-unfixed` or blanket exclusions. Zero findings on this date is bounded dependency evidence, not proof that the complete deployment is secure.
