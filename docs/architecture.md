# Architecture

Maud, Axum, Shared Auth, Turnstile, SeaORM, Supabase, and authenticated
WebSocket Hacker House Medellín web server.

## Browser/auth boundary

The public browser can render marketing, select an individual or organization
journey, and submit a bounded anti-abuse proof. It cannot submit passwords,
product tenant IDs, product roles, provider tokens, or API bearers.

Shared Auth owns the same-origin passwordless ceremony and its configured
`__Host-` session cookie. The Rust server resolves that cookie through
`/auth/browser/session`, rejects ambiguous cookies, wrong provider provenance,
sandboxed/delegated credential classes, and authority outages, then requests
one exact product delegation through `/auth/delegate`.

HHM remains the authority for organization membership, tenant ownership, and
product roles. In particular, Shared Auth `provider_tenant` is only the
credential provider's project/pool namespace.

Browser realtime routes are standardized as `/ws` and `/ws/chat`. Both require
the exact configured origin, a currently resolved interactive session, and the
fixed `hhm:chat:connect` delegation before upgrade.

## Fleet

- `hhm-interfaces`
- `hhm-api`
- `hhm-mash-web`
- `hhm-leptos-web`
- `hhm-dioxus-web`
- `hhm-sync`
- `hhm-cli`
- `hhm-infra`
- `hacker-house-medellin-clients`
- `hacker-house-medellin-libs`
- `hacker-house-medellin.github.io`
- `hacker-house-medellin-monorepo`

Interfaces own wire formats; libraries own reusable domain behavior; clients consume versioned contracts; runtimes own deployment behavior; monorepos coordinate pinned revisions. Edge code is allowlisted and never a generic proxy.
