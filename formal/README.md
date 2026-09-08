# Authenticated member-flow formal verification

`member_intake.qnt` separates browser GET authority from mutation authority. A
verified GET session is required before private prefill data is read or rendered.
Submitting a form requires a fresh POST session decision, exact Origin equality,
and a short-lived delegated token with the exact H/HAUS API audience and
`hhm:intake:write` scope.

For the full application path, the model requires authoritative completion of
the resume upload and then the photo-ID upload before the canonical application
can be submitted. The success page may be rendered only after the API returns an
accepted receipt. Authentication rejection, authority outage, API rejection,
and ambiguous retries cannot create an accepted state.

CI uses Quint 0.32.0 for randomized adversarial schedules and exhaustive TLC
state enumeration under `member_intake_safety`. `check_refinement.py` binds the
model to the actual route handlers, session-cookie parser, Shared Auth delegation
checks, upload client sequence, and error rendering, then emits a digest-bound
JSON receipt.

This proof covers the finite source-level protocol. It does not claim a deployed
Shared Auth, API, PostgreSQL, Supabase, object-storage, Cloudflare, or browser
session has passed a live end-to-end test.
