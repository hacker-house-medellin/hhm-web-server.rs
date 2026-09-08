# Authenticated intake architecture

## Request boundary

`user.hhaus.org` is a server-rendered backend-for-frontend. The browser owns no
API bearer and no Supabase key. A narrow ingress route forwards only these
same-origin ceremony paths to Shared Auth:

- `/shared-auth-ui/`
- `/shared-auth-ui/ui`
- `/shared-auth-ui/auth/browser/sign-in`
- `/shared-auth-ui/auth/browser/consume`
- `/shared-auth-ui/auth/browser/otp`

All other paths go to this web server. Confidential auth routes such as
`/auth/delegate`, `/auth/introspect`, `/auth/exchange`, and `/metrics` remain
cluster-private.

On each page request, the web server forwards exactly one configured host-only
session cookie to Shared Auth's private browser-session resolver. On each
mutation it exchanges that interactive session token through the private
delegation endpoint for a five-minute token limited to the `hhm-api` audience
and `hhm:intake:write` scope. The API independently introspects and rechecks
those constraints.

## Prefill and points

The web server connects with a database role whose default transaction mode is
read-only. `hhm-orm-core::ReadContext` proves that property at startup and
exposes only two named bounded projections: the latest pre-interest prefill and
the current user-points account. Both take a subject created from verified auth
claims; no route accepts a subject from query, path, or form data.

## Private application documents

The application endpoint accepts at most two 10 MB files plus bounded form
overhead. It rejects duplicates, unknown fields, unsafe filenames, disallowed
MIME types, and mismatched file signatures before requesting storage. It hashes
the exact bytes, then performs this flow for each file:

1. authenticated upload intent through the HHaus API;
2. direct `PUT` to the API-issued private Supabase signed URL;
3. authenticated completion request;
4. API streams the stored object and verifies exact owner, object key, kind,
   MIME type, byte count, and SHA-256 against both database metadata records.

Only verified upload UUIDs enter the final application. A retry may encounter
an existing object after an ambiguous upload response; completion verification,
not the upload response, remains authoritative.

## Failure semantics

Invalid Origin, form, nonce, consent, session, or document input creates no
application. A primary API write without a confirmed Supabase mirror returns a
retryable error rather than a success page. Stable nonce-derived idempotency
keys fence exact replays and reject conflicting reuse. Private bytes, identity
claims, cookies, delegated tokens, signed URLs, and detailed upstream response
bodies are never logged.

Expired pending upload metadata and corresponding unreferenced private objects
require a scheduled retention cleanup job. That deployment job is a separate
production gate; this application never grants itself bucket-listing or delete
capabilities.
