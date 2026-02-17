# crate-sentinel-mcp

Rust MCP server for governed dependency upgrades in Rust workspaces.

## Quick Install
```bash
cargo install crate-sentinel-mcp
codex add mcp crate-sentinel crate-sentinel-mcp
```

After this, `crate-sentinel` is registered in Codex as an MCP server.

It is recommended to install additional crates globally in advance to speed up `crate-sentinel` workflows:
```bash
cargo install cargo-semver-checks cargo-public-api cargo-download
```

## What It Does
- Discovers external dependencies (`deps.scan`)
- Checks latest versions and policy allow/deny (`deps.check_updates`)
- Simulates upgrades in isolated copies (`deps.try_upgrade`)
- Detects breaking API changes (`deps.api_diff`)
- Applies controlled real upgrades (`deps.apply_upgrade`)
- Supports refactor planning/validation bridge (`refactor.plan`, `refactor.validate`)
- Supports CI summaries (`ci.report`)

## Governance Model
- Policy file: `upgrade_policy.toml`
- Default denies major updates, allows patch/minor
- Optional guards:
  - MSRV guard
  - Performance regression guard
  - CI fail-on-disallowed mode

## Session Model
- Start a session with `session.start`
- One active session per workspace lock
- Session tracks current crate, state, and audit events
- End each active session with `session.end` to remove persisted session files
- Recover persisted sessions with `session.recover`

## Policy Example
```toml
[general]
allow_patch = true
allow_minor = true
allow_major = false
include_dev_dependencies = false

[msrv]
enforce = true
max_allowed = "1.85"

[performance]
enforce = false
command = "cargo bench -p mybenchcrate"
max_slowdown_percent = 5

[ci]
mode = true
fail_on_any_disallowed_update = true
```

## CI Usage
```bash
crate-sentinel-mcp --ci --workspace .
```

Exit codes:
- `0` success
- `1` validation/recoverable failure
- `2` policy violation
- `3` internal/runtime failure

## Refactor Bridge Workflow
1. `deps.api_diff`
2. `refactor.plan`
3. LLM proposes unified diff patch
4. `refactor.validate`
5. `deps.try_upgrade`
6. `deps.apply_upgrade`

`deps.apply_upgrade` is blocked unless required gates pass.
