# Intake cross-surface decision

The initial requested delivery is browser-only: public marketing forms and the
authenticated `user.hhaus.org` backend-for-frontend. Native Flutter and Tauri
screens are intentionally unchanged in this pull request because the private
resume/photo-ID upload ceremony, consent copy, retention UX, and authenticated
browser handoff require their own reviewed platform implementations.

The cross-surface contract is not browser-specific. `hhm-interfaces` owns the
application, referral, upload-intent, upload-completion, receipt, and validation
types, so future native clients must consume the same versioned fields and
fixtures. A native parity follow-up must preserve:

- Shared Auth subject ownership and exact `hhm-api` delegation;
- fresh anonymous anti-automation proof or authenticated-session alternative;
- private signed uploads with exact digest/size/MIME verification;
- the same age, identity, consent, and privacy-notice versions;
- dual-persistence receipts and idempotent replay behavior;
- secure local file pickers with no credentials or private document data in
  deep links, notifications, analytics, crash reports, or logs.

This note is the explicit no-native-change rationale, not a claim of Flutter or
desktop parity.
