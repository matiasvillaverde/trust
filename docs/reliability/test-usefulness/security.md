# Test usefulness: security

Baseline `3e8718f31bf0` · 4 entries · [Method and score definitions](../test-usefulness.md) · [Complete sortable CSV](catalog.csv).

**Estimates are review judgments, not measured defect-detection rates.** Low-confidence indexed rows need deeper review. Ignored/external cases contribute no default-run evidence. See the CSV for full IDs, source links, helper provenance, and status.

Actions are defined in the [catalog guide](../test-usefulness.md#actions). `Indexed` is automated source-assisted triage; `Family review` covers a shared oracle, not a separate proof of every case.

## [security/tests/private_storage.rs](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/security/tests/private_storage.rs)

| Test / source | Usefulness | Confidence / basis | Evidence and limitation | Action |
| --- | --- | --- | --- | --- |
| secret_permissions_links_special_files_and_bounds_fail_closed [L15](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/security/tests/private_storage.rs#L15) | 81–95% Strong | Medium / Family review | Real file/permission/key handling enforces a security boundary, including positive RAM-file admission.<br><code>assert!(read_secret(&fifo, 32).is_err())</code> | KEEP |
| private_directories_and_keys_never_overwrite_or_accept_bad_formats [L58](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/security/tests/private_storage.rs#L58) | 81–95% Strong | Medium / Family review | Real file/permission/key handling enforces a security boundary, including positive RAM-file admission.<br><code>assert!(private_file(&public.join("key"), true).is_err())</code> | KEEP |
| production_accepts_ram_mounted_secrets [L94](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/security/tests/private_storage.rs#L94) | 81–95% Strong | Medium / Family review | Real file/permission/key handling enforces a security boundary, including positive RAM-file admission.<br><code>A generated private key on /dev/shm must pass require_memory_file(...).unwrap().</code> | KEEP |
| native_backend_requires_patched_stable_sqlcipher_four [L103](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/security/tests/private_storage.rs#L103) | 81–95% Strong | Medium / Family review | Real file/permission/key handling enforces a security boundary, including positive RAM-file admission.<br><code>assert!(trust_security::supported_sqlcipher_version("4.20.1"))</code> | KEEP |
