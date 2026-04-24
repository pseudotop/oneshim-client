# External gRPC Binding Guide

## Overview

External gRPC binding lets the desktop agent accept connections from outside the loopback interface (127.0.0.1). This enables target use cases such as LAN dashboard access, remote team monitoring, and integration with central management systems. The feature is opt-in via the `external_grpc.enabled: true` configuration flag, keeping the default behavior unchanged (zero impact on existing deployments). Both TLS and JWT or mTLS authentication are required for security; this is not optional.

## Setup

### Certificate Generation

Use the `generate-external-cert` CLI (argv-dispatched from the Tauri main binary)
to produce a complete TLS + JWT key bundle:

```bash
cargo run -p oneshim-app --features external-grpc-tools -- generate-external-cert \
    --output-dir ~/.oneshim/certs/ --bind-ip 0.0.0.0
```

This command produces four files in the output directory:

- `server.crt` — TLS server certificate (1-year self-signed, bound to the `--bind-ip`)
- `server.key` — TLS server private key (PKCS#8 format, unencrypted)
- `jwt_signing.pub` — JWT public key for verification (ES256 or RSA-2048, depending on the algorithm selected during generation)
- `jwt_signing.priv` — JWT private key for signing (kept on the agent; also copy to your central auth service if issuing tokens remotely)

**Key distribution:**

- `server.crt` and `server.key` remain on the agent's filesystem.
- `jwt_signing.pub` is placed on the agent (for local JWT verification if using local signing).
- `jwt_signing.priv` is distributed to your central authentication service only if that service will mint tokens; otherwise, keep it private to the agent.
- Keep `server.key` confidential and backed up separately from `server.crt`.

### Configuration

Add the following section to the agent's configuration file (TOML format):

```toml
[external_grpc]
enabled = true
bind_address = "0.0.0.0"
port = 10092
tls_cert_path = "/path/to/server.crt"
tls_key_path = "/path/to/server.key"
auth_mode = "jwt"
jwt_algorithm = "ES256"
jwt_public_key_path = "/path/to/jwt_signing.pub"
jwt_expected_issuer = "your-auth-service"
jwt_expected_audience = "oneshim-agent-{device_id}"
```

**Configuration fields:**

- `enabled` — Boolean. Set to `true` to activate the external server; default is `false`.
- `bind_address` — String. IP address to bind to. Use `"0.0.0.0"` for all interfaces, or a specific IP like `"192.168.1.100"`.
- `port` — Integer. Port number (1024–65535). Default is 10092.
- `tls_cert_path` — String. Absolute path to the TLS certificate file.
- `tls_key_path` — String. Absolute path to the TLS private key file.
- `auth_mode` — String. One of `"jwt"`, `"mtls"`, or `"jwt+mtls"`. Determines which authentication methods are accepted.
- `jwt_algorithm` — String. One of `"ES256"` (ECDP-256, 64-byte signature) or `"RS256"` (RSA-2048, 256-byte signature). Must match the algorithm used when generating `jwt_signing.pub`.
- `jwt_public_key_path` — String. Absolute path to the JWT public key for verification.
- `jwt_expected_issuer` — String. Expected `iss` claim in incoming JWTs. Tokens with a different issuer are rejected.
- `jwt_expected_audience` — String. Expected `aud` claim. Use placeholders like `{device_id}` which are interpolated at startup.

### Firewall

Open the configured port on your system firewall:

**macOS (App Firewall):**
```bash
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --add /path/to/oneshim-app
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --unblockapp /path/to/oneshim-app
```

**Windows (Windows Defender Firewall):**
```powershell
New-NetFirewallRule -DisplayName "ONESHIM gRPC" -Direction Inbound `
    -Program "C:\path\to\oneshim-app.exe" -Action Allow -Protocol TCP -LocalPort 10092
```

**Linux (UFW):**
```bash
sudo ufw allow 10092/tcp
sudo ufw reload
```

## Reverse Proxy Examples

External gRPC traffic is typically exposed through a reverse proxy for domain routing, rate limiting, and WAF integration.

### Caddy

Simple and automatic HTTPS:

> ⚠️ **Security note**: `tls_insecure_skip_verify` disables certificate verification between
> Caddy and the agent. Safe only when Caddy and the agent are on the same host (e.g., Caddy
> as a sidecar on localhost). For cross-host deployments, remove the flag and provide Caddy
> with the agent's cert (copy `server.crt` to Caddy's trust store OR use `transport http { tls_trusted_ca_certs server.crt }`).

```caddy
oneshim.example.com:443 {
    reverse_proxy localhost:10092 {
        transport http {
            tls
            tls_insecure_skip_verify  # use only with self-signed certs; require CA verification in production
        }
    }
}
```

### Nginx (Stream Module)

> ⚠️ **Note**: Nginx `stream` is TCP pass-through — no L7 features (HTTP-level routing,
> auth headers, rewrite rules) work. The agent terminates TLS + gRPC directly.

```nginx
stream {
    upstream oneshim_backend {
        server 127.0.0.1:10092;
    }
    
    server {
        listen 443;
        listen [::]:443;
        proxy_pass oneshim_backend;
    }
}
```

### Cloudflare Tunnel

No public IP required; Cloudflare handles authentication and encryption.

Persistent tunnel config (`~/.cloudflared/config.yml`):
```yaml
tunnel: <your-tunnel-uuid>
credentials-file: /path/to/<uuid>.json
ingress:
  - hostname: oneshim.example.com
    service: https://localhost:10092
    originRequest:
      noTLSVerify: true  # remove for production with proper certs
  - service: http_status:404
```

Then create a DNS CNAME or use Cloudflare's routing rules to point `oneshim.example.com` to the tunnel.

## Security Checklist

Use this checklist to validate your external gRPC deployment:

- [ ] **TLS certificate rotated within 365 days.** The agent hot-reloads certs via file watcher (atomic rename); no restart needed.
- [ ] **JWT signing key pair rotated at least annually.** The agent requires a restart to pick up a new public key; plan maintenance accordingly.
- [ ] **mTLS client certificates lifetime capped at 48 hours.** The agent rejects longer-lived certs.
- [ ] **mTLS fingerprint allowlist populated** if deploying to a multi-team fleet (e.g., restrict to team-A's CI/CD runners).
- [ ] **IP ban thresholds reviewed** against expected traffic patterns. Default: ban after 5 failed auth attempts per IP, with exponential backoff (60s → 10min → 1hr).
- [ ] **Audit log queried periodically.** Agent writes a local audit trail to the SQLite database; periodic manual review or automated export recommended (see "Auditing" section below).
- [ ] **TLS cipher suites validated** against your security policy (consult `rustls` defaults and your compliance requirements).
- [ ] **Reverse proxy logging enabled** and monitored for unexpected patterns (e.g., port scans, brute-force auth attempts).

## Auditing

Every external gRPC request produces a Started + Completed pair in the agent's
local audit database. `AuthLayer` writes Started on auth success and Failed on
rejection (per-reason: `invalid_jwt`, `missing_token`, `fingerprint_mismatch`,
`missing_cert`); `AuditLayer` (inner of `AuthLayer`) writes Completed after the
handler returns. Query surfaces:

- `entries_by_status(AuditStatus::Completed, N)` — successful RPCs.
- `entries_by_status(AuditStatus::Failed, N)` — auth rejections.
- `entries_by_action_prefix("external_grpc_", N)` — all external rows
  (`external_grpc_started`, `external_grpc_completed`, `external_grpc_failed`,
  `external_grpc_denied`, `external_grpc_timeout`).

All external gRPC requests are logged to the agent's local audit database with the following details:

- `timestamp` — Request arrival time (UTC).
- `peer_ip` — Client IP address and port.
- `peer_cert_cn` — Certificate Common Name (if mTLS).
- `peer_cert_fingerprint` — SHA-256 of the peer cert (if mTLS).
- `jwt_issuer` — JWT `iss` claim.
- `jwt_subject` — JWT `sub` claim.
- `request_type` — gRPC method name (e.g., `/oneshim.v1.DashboardService/GetSessionStats`).
- `status_code` — gRPC status (OK, Unauthenticated, PermissionDenied, etc.).
- `error_detail` — Reason for rejection (if applicable).

To export the audit log:

```bash
# REST API endpoint (local only, no auth required)
curl http://localhost:10090/api/audit/export?days=7 > audit-7d.json
```

To query via the CLI:

```bash
sqlite3 ~/.oneshim/oneshim.db "SELECT * FROM audit_log WHERE timestamp > datetime('now', '-7 days') ORDER BY timestamp DESC LIMIT 100;"
```

## Troubleshooting

### Connection Refused to Port 10092

**Symptom:** `connection refused` when connecting to the agent's gRPC endpoint.

**Diagnosis:**
1. Verify the config flag: `external_grpc.enabled = true`.
2. Check firewall: `lsof -i :10092` (macOS/Linux) or `netstat -ano | findstr :10092` (Windows).
3. Check agent logs for "port in use" error: `grep -i "port\|address" ~/.oneshim/agent.log`.

**Fix:**
- If the port is in use by another process, either stop that process or change the `external_grpc.port` setting.
- Ensure the `bind_address` matches your network configuration (use `0.0.0.0` for all interfaces).

### TLS Handshake Failed

**Symptom:** `tls: handshake failure` or `x509: certificate verify failed` in client logs.

**Diagnosis:**
1. Verify cert/key paths are correct: `ls -la /path/to/server.crt /path/to/server.key`.
2. Verify cert and key are a matching pair: `openssl x509 -in server.crt -text -noout` and `openssl pkey -in server.key -text -noout` should have matching modulus (RSA) or public point (ECDSA).
3. Check certificate expiry: `openssl x509 -enddate -noout -in server.crt`.

**Fix:**
- Regenerate the cert pair: `cargo run -p oneshim-app --features external-grpc-tools -- generate-external-cert --output-dir ~/.oneshim/certs/ --bind-ip 0.0.0.0`
- Update the config paths and restart the agent.
- For development with self-signed certs, clients must allow `tls_insecure_skip_verify` (equivalent to curl `-k`).

### Unauthenticated (JWT or mTLS)

**Symptom:** `rpc error: code = Unauthenticated desc = invalid token` or `cert not allowed`.

**Diagnosis:**
1. Verify the JWT is present and well-formed:
   ```bash
   echo "<token>" | jq .  # should parse without error
   ```
2. Verify claims: `echo "<token>" | jq '.iss, .aud, .exp'`.
3. Check the agent config: `grep jwt_expected_ ~/.oneshim/config.toml`.

**Fix:**
- Ensure the `Authorization: Bearer <token>` header is included in the gRPC request (note: gRPC uses custom metadata, not HTTP headers; your client library must map this).
- Verify the issuer and audience claims match the config exactly (case-sensitive).
- Check token expiry: if `exp` is in the past, obtain a fresh token.
- For mTLS: verify the client cert is in the allowlist and has not expired.

### IP Banned

**Symptom:** `rpc error: code = Unavailable desc = ip banned` after a few connection attempts.

**Diagnosis:**
1. The agent tracks failed auth attempts per IP address. After 5 consecutive failures, the IP is banned for 60 seconds.
2. Subsequent bans (if the IP fails again) increase the backoff: 10 minutes, then 1 hour.

**Fix:**
- Wait for the backoff period to expire (shown in the agent log as `external_grpc: IP 192.168.1.100 banned until 2026-04-23T10:30:00Z`).
- Fix the authentication issue (token, cert, etc.) and retry.
- To immediately unban an IP, restart the agent (in-memory ban state is cleared).

### Certificate Expiry Warning in Log

**Symptom:** Agent logs `external_grpc: TLS cert expires in 3 days` (or similar).

**Diagnosis:**
The agent checks certificate expiry at startup and logs warnings if the cert expires within 7 days.

**Fix:**
- Regenerate the cert immediately:
  ```bash
  cargo run -p oneshim-app --features external-grpc-tools -- generate-external-cert \
      --output-dir ~/.oneshim/certs/ --bind-ip 0.0.0.0 --force
  ```
- The agent will hot-reload the new cert within seconds (via file watcher).
- No restart required.

## Advanced Configuration

### mTLS Client Certificate Fingerprint Allowlist

If you need to restrict which client certificates are allowed, configure the allowlist:

```toml
[external_grpc]
mtls_fingerprint_allowlist = [
    "SHA256:abc123def456...",  # team-a-ci-runner
    "SHA256:xyz789uvw012...",  # team-b-automation
]
```

The agent computes the SHA-256 fingerprint of each peer certificate and rejects connections from certificates not in the list. Leave empty to accept all valid mTLS certs.

### JWT Token Refresh

For long-lived connections (e.g., continuous streaming), ensure your token refresh cadence is shorter than the token lifetime. Example:

```bash
# Token lifetime: 1 hour
# Refresh every 50 minutes
while true; do
    TOKEN=$(curl -X POST https://auth.example.com/token \
        -H "Content-Type: application/json" \
        -d '{"client_id":"...","client_secret":"..."}' | jq -r .access_token)
    grpcurl -H "authorization: Bearer $TOKEN" \
        localhost:10092 list oneshim.v1.DashboardService
    sleep 3000  # 50 minutes
done
```

### Monitoring and Alerts

Set up monitoring on the audit log to detect suspicious patterns:

```bash
# Query failed auth attempts in the last hour
sqlite3 ~/.oneshim/oneshim.db \
    "SELECT peer_ip, COUNT(*) as failures FROM audit_log \
     WHERE status_code != 'OK' AND timestamp > datetime('now', '-1 hour') \
     GROUP BY peer_ip ORDER BY failures DESC;"
```

Alert if:
- Any IP has > 10 failed auth attempts per hour.
- Any new peer certificate appears suddenly.
- Token `iss` or `aud` claims change unexpectedly.

## See Also

- [gRPC Client Guide](grpc-client.md) — Connecting to gRPC endpoints (internal and external).
- [gRPC Governance](grpc-governance.md) — RPC versioning and API stability policy.
- [gRPC Error Mapping](grpc-error-mapping.md) — Understanding gRPC error codes.
- [Enterprise Deployment](enterprise-deployment.md) — Scaling the agent across a fleet.

## Running stress tests locally

The external gRPC stress suite (`crates/oneshim-web/tests/external_grpc_stress.rs`) is gated behind the `stress-test` cargo feature so it never runs in the regular `cargo test --workspace` path. The suite covers three scenarios:

1. `concurrent_connection_cap_enforced` — 1024 concurrent connections at `max_connections = 1024`, slot-recovery on drop.
2. `fd_pressure_resilience` — 3 rounds of 1024-stream churn, no fd leak post-loop.
3. `ipv6_64_prefix_ban_full_stack` — `IpBan` accept-loop wiring on `[::1]` (5 auth failures → 6th rejected pre-TLS).

### Local prerequisites

- `ulimit -n 65536` (raise the open-file limit before invoking cargo).
- IPv6 loopback (`[::1]`) reachable. Default on Linux/macOS.
- ~5s to ~15s per test on modern hardware.

### Command

```sh
ulimit -n 65536
cargo test -p oneshim-web --features stress-test \
  --test external_grpc_stress \
  -- --test-threads=1 --nocapture
```

`--test-threads=1` is mandatory — Tests 1 and 2 each consume ~2050 file descriptors. Running them in parallel needs >4000 fds AND increases racy cleanup paths.

### CI invocation

Stress tests run via the `gRPC Stress Test` workflow (`.github/workflows/grpc-stress.yml`):

- Manually: `gh workflow run grpc-stress.yml --ref <branch>`.
- Weekly: every Sunday 03:00 UTC.

The workflow runs on `ubuntu-latest` (only platform with predictable `ulimit -n` and IPv6 loopback semantics).
