#!/usr/bin/env python3
import json, re
from pathlib import Path
root = Path(__file__).resolve().parents[1]
meta = json.loads((root / "project.json").read_text())
required = ["README.md", "AGENTS.md", "project.json", "docs/architecture.md", *meta.get("required_paths", [])]
missing = [path for path in required if not (root / path).exists()]
if missing: raise SystemExit(f"missing required paths: {missing}")
for path in root.rglob("*"):
    if not path.is_file() or ".git" in path.parts or path.stat().st_size > 1_000_000: continue
    try: text = path.read_text()
    except UnicodeDecodeError: continue
    if any(marker in text for marker in ("<"*7, "="*7, ">"*7)): raise SystemExit(f"conflict marker in {path}")
    if re.search(r"gh[pousr]_[A-Za-z0-9]{20,}|lin_api_[A-Za-z0-9]{20,}|BEGIN [A-Z ]*PRIVATE KEY", text):
        raise SystemExit(f"credential-shaped content in {path}")
lockfile = (root / "Cargo.lock").read_text()
git_sources = set(re.findall(r'^source = "(git\+[^\"]+)"$', lockfile, re.MULTILINE))
allowed_git_sources = {
    "git+https://github.com/flags-2-env/flags-2-env?rev=d7bad9ea7dfc657653368237e08903514cdca1ec#d7bad9ea7dfc657653368237e08903514cdca1ec"
}
unexpected_git_sources = sorted(git_sources - allowed_git_sources)
if unexpected_git_sources:
    raise SystemExit(f"unapproved git dependency sources: {unexpected_git_sources}")
print(f"validated {meta['organization']}/{meta['repository']}")
