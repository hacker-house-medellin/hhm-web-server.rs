#!/usr/bin/env python3
"""Bind the member-intake model to the production session and form flow."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LIB = ROOT / "src" / "lib.rs"
AUTH = ROOT / "src" / "auth.rs"
API = ROOT / "src" / "api.rs"
MODEL = ROOT / "formal" / "member_intake.qnt"


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require(text: str, fragment: str, label: str) -> None:
    if fragment not in text:
        raise SystemExit(f"missing member-intake refinement anchor: {label}")


def require_order(text: str, items: list[tuple[str, str]], label: str) -> None:
    positions: list[int] = []
    for name, fragment in items:
        position = text.find(fragment)
        if position < 0:
            raise SystemExit(f"missing {label} refinement anchor: {name}")
        positions.append(position)
    if positions != sorted(positions):
        raise SystemExit(f"production order no longer refines {label}")


def function_body(text: str, start: str, end: str) -> str:
    begin = text.find(start)
    if begin < 0:
        raise SystemExit(f"missing function boundary: {start}")
    finish = text.find(end, begin + len(start))
    if finish < 0:
        raise SystemExit(f"missing function boundary: {end}")
    return text[begin:finish]


def main() -> None:
    lib = LIB.read_text(encoding="utf-8")
    auth = AUTH.read_text(encoding="utf-8")
    api = API.read_text(encoding="utf-8")
    model = MODEL.read_text(encoding="utf-8")

    show_application = function_body(
        lib,
        "async fn show_application",
        "async fn submit_application",
    )
    require_order(
        show_application,
        [
            ("fresh GET session", "require_session_for_get"),
            ("private prefill", "load_prefill"),
            ("render form", "views::application"),
        ],
        "application GET",
    )

    submit_application = function_body(
        lib,
        "async fn submit_application",
        "async fn show_referral",
    )
    require_order(
        submit_application,
        [
            ("exact Origin", "valid_post_origin"),
            ("fresh POST session", "require_session_for_post"),
            ("bounded multipart parse", "forms::parse_application"),
            ("write-only delegation", "delegate_for_intake"),
            ("resume completion authority", '"application:{}:resume"'),
            ("photo completion authority", '"application:{}:photo-id"'),
            ("bind upload ids", "parsed.input.resume_upload_id = resume_id"),
            ("validate completed contract", "parsed.input.validate()"),
            ("submit canonical application", ".submit_application("),
            ("show accepted receipt", 'views::success("Application"'),
        ],
        "application POST",
    )

    lib_anchors = {
        "post origin exactness": "value == expected",
        "GET auth before private data": "require_session_for_get",
        "POST auth is independently resolved": "require_session_for_post",
        "local return path": "shared_auth_browser_prefix",
        "API failure does not claim completion": "Submission not completed",
        "dual persistence message": "could not confirm both HHaus database copies",
    }
    for label, fragment in lib_anchors.items():
        require(lib, fragment, label)

    auth_anchors = {
        "one cookie header": "if values.len() > 1",
        "duplicate cookie refusal": "if found.is_some()",
        "AAL admission": "claims.aal == 0",
        "credential claim rejection": "claims.cred.is_some()",
        "verified subject constructor": "VerifiedSubject::from_verified_claim",
        "delegation scope": 'scopes: ["hhm:intake:write"]',
        "delegated audience equality": "delegated.audience != self.api_audience",
        "delegated scope equality": 'scope == "hhm:intake:write"',
        "bounded token": "delegated.access_token.len() > MAX_TOKEN_BYTES",
        "redirect refusal": ".redirect(reqwest::redirect::Policy::none())",
    }
    for label, fragment in auth_anchors.items():
        require(auth, fragment, label)

    require_order(
        api,
        [
            ("upload intent", '"v1/intake/uploads"'),
            ("storage origin check", "intent.upload_url.origin() != self.storage_origin"),
            ("object transfer", ".put(intent.upload_url)"),
            ("completion authority", '"v1/intake/uploads/{}/complete"'),
            ("receipt id equality", "completed.upload_id != intent.upload_id"),
        ],
        "private upload client",
    )

    model_anchors = {
        "private read invariant": "private_reads_require_get_session",
        "fresh post invariant": "mutations_require_fresh_post_authority",
        "upload sequence invariant": "upload_and_application_order",
        "fail-closed invariant": "failures_do_not_create_acceptance",
        "aggregate invariant": "member_intake_safety",
    }
    for label, fragment in model_anchors.items():
        require(model, fragment, label)

    print(
        json.dumps(
            {
                "schema": "hhaus.member-formal-refinement.v1",
                "model": "member_intake",
                "invariant": "member_intake_safety",
                "libSha256": digest(LIB),
                "authSha256": digest(AUTH),
                "apiClientSha256": digest(API),
                "modelSha256": digest(MODEL),
            },
            sort_keys=True,
            separators=(",", ":"),
        )
    )


if __name__ == "__main__":
    main()
