# hhm-web-server.rs

**Hacker House Medellín — public and authenticated web server: Maud + Axum + Shared Auth + Turnstile + WebSockets**

Operations and community software for an entrepreneur-focused coliving and coworking house in Medellín, Colombia.

This repository was bootstrapped on 2026-08-04. It is designed as an independently deployable component and as a member of the `hhm-monorepo` workspace.

## GitHub target

`hacker-house-medellin/hhm-web-server.rs`

## Public account entry

The server now owns polished, responsive entry surfaces for two separate journeys:

- `/join/individual` for a person planning a one-time or continuing HHaus stay;
- `/join/organization` for an accountable person starting a team or organization program.

Both use the same safe boundary. The entry POST requires an exact `Origin`, a
short-lived `Secure; HttpOnly; SameSite=Lax` CSRF cookie, and a Cloudflare
Turnstile result bound to the configured HHaus hostname and action. It then
redirects only to the allow-listed same-origin Shared Auth browser prefix.

HHaus never accepts a password on these pages. Browser JavaScript never receives
a provider token, Shared Auth token, or delegated HHM API bearer. The web server
resolves the host-only Shared Auth session over a private back channel and asks
for one fixed audience/scope delegation before showing an onboarding shell or
opening realtime transport.

Provider provenance such as `provider_tenant` is checked against runtime
configuration but is never treated as an HHaus organization. Organization,
membership, and product-role authority must be derived from HHM-owned server
records; no browser form in this slice accepts those claims.

See [`docs/auth-browser-boundary.md`](docs/auth-browser-boundary.md).

## Development

```bash
cp .env.example .env 2>/dev/null || true
nix develop  # optional
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/verify_repo.py
```

## Status

This repository contains source and test coverage for the public auth/UI slice.
It does not claim that Shared Auth proxy routes, Turnstile keys, delegation
policies, DNS, Kubernetes, or a provider deployment are configured. Realtime
routes `/ws` and `/ws/chat` remain closed until the exact browser origin,
Shared Auth session, and `hhm:chat:connect` delegation are all accepted.

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
