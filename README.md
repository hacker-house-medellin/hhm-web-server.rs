# hhm-web-server.rs

**Hacker House Medellín — MASH web server: Maud + Axum + SeaORM + Supabase + HTMX + WebSockets**

Operations and community software for an entrepreneur-focused coliving and coworking house in Medellín, Colombia.

This repository was bootstrapped on 2026-08-04. It is designed as an independently deployable component and as a member of the `hhm-monorepo` workspace.

## GitHub target

`hacker-house-medellin/hhm-web-server.rs`

## Baseline

- Rust 2024 edition for backend and native components.
- Axum HTTP/WebSocket transport.
- Supabase/PostgreSQL configuration through `DATABASE_URL`, `SUPABASE_URL`, and environment-only secrets.
- OpenTelemetry-compatible tracing hooks.
- Docker, Nix, and GitHub Actions entry points.
- Contracts live in `hhm-interfaces`; shared behavior lives in `hhm-libs`.

## Development

```bash
cp .env.example .env 2>/dev/null || true
nix develop  # optional
cargo fmt --check 2>/dev/null || true
cargo test 2>/dev/null || true
```

## Status

Foundation scaffold. Domain behavior, persistence migrations, authentication policy, and production secrets must be reviewed before deployment.

## Cross-surface delivery

User-visible, membership, application, room, booking, event, payment, message,
notification, permission, navigation, or deep-link changes in this Rust web
server must be evaluated for:

- `hacker-house-medellin/hhm-flutter` on Android, iOS, Flutter Web/mobile web,
  and Flutter desktop;
- `hacker-house-medellin/hhm-desktop.rs`, the planned Tauri 2 desktop app; and
- `hhm-interfaces`, generated clients, membership/room/event/payment schemas,
  route types, offline fixtures, and conformance tests.

This is judgment-based coordination. Public marketing, SEO, and browser-only
administration may remain web-only. Native check-in, printing, local files,
secure storage, notifications, and offline organizer workflows may be
native-specific. Membership/account status, applications, bookings, event
state, payment status, messaging, permissions, errors, and navigation normally
require coordinated updates or an explicit no-change rationale and parity
follow-up.

Deep links are HTTPS-first:

```text
https://<verified-hacker-house-medellin-owned-host>/open/<route>?<bounded-query>
```

with `hhm://` fallback. MASH web, Flutter, and Tauri desktop must share
versioned route types and fixtures and support cold start, already-running
delivery, authentication resume, replay/expiry rejection, and browser fallback.
Payment credentials, access codes, private member data, room-entry secrets,
message contents, provider credentials, and bearer/refresh tokens are
prohibited in URLs. Applications, invitations, bookings, check-in, and payment
handoffs use bounded identifiers or short-lived, single-use, audience-bound
codes and explicit confirmation.

See [`docs/CROSS_SURFACE_DELIVERY.md`](docs/CROSS_SURFACE_DELIVERY.md) and the
[portfolio policy](https://github.com/ORESoftware/project-registry/blob/main/docs/cross-surface-delivery.md).

## Environment secrets

Secrets live in this repo **encrypted** with [sops](https://github.com/getsops/sops) + [age](https://github.com/FiloSottile/age):
`env/enc/<dev|prod>.env.enc` is committed; `just env-use <name>` decrypts it to
`env/dec/<name>.env` (gitignored, mode 0600) and symlinks `./.env` to it. The
Nix dev shell provides the tooling, `just env-audit` runs keyless in CI, and
containers decrypt at `docker run` — never at build. See [`env/README.md`](env/README.md).
