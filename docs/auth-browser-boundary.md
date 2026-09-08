# HHaus browser authentication boundary

This slice uses Shared Auth's same-origin proxied browser ceremony. It does not
implement a password form or a second provider login flow.

## Route contract

| Route | Exposure | Security decision |
| --- | --- | --- |
| `GET /join/individual` | Public | Issues a short-lived host-only CSRF cookie and renders Turnstile. |
| `GET /join/organization` | Public | Same boundary; it does not accept an organization or role claim. |
| `POST /auth/start` | Public mutation | Exact origin + constant-time CSRF match + Turnstile hostname/action verification. |
| `/shared-auth-ui/auth/browser/*` | Gateway-owned narrow proxy | Shared Auth owns sign-in, OTP consumption, and session cookies. No catch-all proxy is authorized by this repository. |
| `GET /onboarding/{kind}` | Authenticated | Resolves the session server-to-server and requests one fixed HHM API scope. |
| `GET /ws`, `GET /ws/chat` | Authenticated upgrade | Opens only after origin, session, and `hhm:chat:connect` delegation succeed. |

The only allowed post-sign-in returns created by this server are:

- `/onboarding/individual`
- `/onboarding/organization`

Absolute URLs, protocol-relative URLs, client-provided return paths, and
organization identifiers are not accepted by the auth-start form.

## Cookie and token handling

- Entry CSRF: `__Host-hhaus-entry-csrf`, `Secure`, `HttpOnly`, `SameSite=Lax`,
  `Path=/`, ten-minute maximum age, cleared after an attempt.
- Shared Auth session: exact runtime-configured `__Host-` name; duplicate cookie
  fields and multiple Cookie headers fail closed.
- Delegated bearer: fixed client, audience, and one enumerated scope; bounded to
  a maximum fifteen-minute response lifetime and retained only in server memory.
- No session or delegated bearer is written to HTML, JSON, URLs, logs, browser
  storage, or WebSocket messages.

## Authority split

Shared Auth proves a canonical user and assurance provenance. Its provider,
provider-tenant, and role values are authentication metadata. They do not create
an HHaus organization, membership, invitation, billing grant, room permission,
or product role.

The organization onboarding page intentionally stops at an authenticated shell.
A subsequent HHM API slice must derive organization and role authority from
HHM-owned persistent records, require the exact delegated audience/scope, and
add negative cross-tenant tests before accepting organization mutations.

## Deployment gates not satisfied by source publication

- narrow gateway proxy allow-list for Shared Auth browser routes;
- production Turnstile site/secret registration and hostname read-back;
- Shared Auth delegation policies for the five enumerated HHM scopes;
- exact provider-tenant and session-cookie configuration;
- outside-in DNS/TLS/runtime verification;
- authenticated `/ws` and `/ws/chat` canary;
- provider and device acceptance.
