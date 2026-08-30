# hhm-web-server.rs

Authenticated server-rendered web application for `user.hhaus.org`. It provides
prefilled pre-interest and application forms, authenticated referrals, and an
HHaus points summary without exposing browser tokens or database credentials to
client-side JavaScript.

## Routes

| Method | Route | Purpose |
| --- | --- | --- |
| `GET` | `/` | Account home and points balance |
| `GET/POST` | `/submit-pre-interest` | Prefilled founder pre-interest form |
| `GET/POST` | `/submit-application` | Full application with private resume and photo-ID upload |
| `GET/POST` | `/submit-referral` | Referral tied to the signed-in subject |
| `GET` | `/healthz` | Process liveness |
| `GET` | `/readyz` | Read-only database readiness |

Every account route requires the exact host-only Shared Auth cookie configured
by `SHARED_AUTH_SESSION_COOKIE_NAME`. Signed-out `GET` requests redirect to the
narrow same-origin browser ceremony under `/shared-auth-ui`; failed or expired
sessions never reveal prefill, points, or form receipts.

## Security model

- The ingress sends only the allow-listed Shared Auth browser ceremony routes
  under `/shared-auth-ui` to Shared Auth. It must never expose a catch-all auth
  proxy or confidential introspection/delegation endpoints.
- The server resolves the browser cookie through Shared Auth, rejects duplicate
  cookies and machine credentials, then delegates the session to the exact
  `hhm-api` audience and `hhm:intake:write` scope.
- The delegated bearer exists only in server memory. It is never rendered into
  HTML, URLs, browser storage, JavaScript, or logs.
- Prefill and points use `hhm-orm-core`'s opaque `ReadContext`; startup fails
  unless PostgreSQL proves the role is transaction-read-only.
- Every form mutation requires the exact `https://user.hhaus.org` Origin and an
  authenticated session. Body subjects are absent from the shared contracts;
  the API attaches the verified Shared Auth subject.
- Resume and photo-ID bodies are bounded to 10 MB each, checked against
  kind-specific file signatures, hashed, uploaded to the private Supabase
  bucket, then streamed and independently verified by the API before the full
  application can reference them.
- All account responses are private/no-store and use a restrictive CSP,
  `nosniff`, no-referrer, anti-framing, HSTS, and a narrow Permissions Policy.

## Dual persistence

The web server never writes either database itself. It submits versioned
contracts to `hhm-api-server.rs`, which commits a primary PostgreSQL row and
outbox item, mirrors the same UUID and payload digest into the dedicated HHaus
Supabase PostgreSQL project, and acknowledges success only after both copies
are present. Retried form nonces produce stable idempotency keys.

## Runtime configuration

See `.env.example` for the complete list. `READ_DATABASE_URL` must belong to a
dedicated read-only role. `API_BASE_URL` and `SHARED_AUTH_BASE_URL` should be
cluster service roots; public cleartext HTTP is rejected. `SUPABASE_URL` is used
only to pin the allowed origin of API-issued signed upload URLs.

The deployment must configure Shared Auth consistently:

- browser public prefix: `/shared-auth-ui`;
- session cookie: the exact `__Host-` name configured here;
- a delegation policy for `client_id=hhm-user-web`, `audience=hhm-api`, and
  `allowed_scopes=["hhm:intake:write"]`.

## Development and verification

Rust 1.94 or newer is required.

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo test --locked --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
```

The unit suite uses no credentials. A deployed acceptance run additionally
needs the reviewed schema on both databases, the private Storage bucket, the
same-origin Shared Auth proxy, and live `api.hhaus.org` routing.

Encrypted runtime environment handling remains documented in
[`env/README.md`](env/README.md).
