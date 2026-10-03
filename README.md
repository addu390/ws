# WS MCP

Unofficial Wealthsimple MCP server in Rust. Not affiliated with or endorsed by Wealthsimple.

Wealthsimple has no public API. This talks to the same GraphQL API as the web app, which can change without notice, and automated access may conflict with Wealthsimple's terms. Use at your own risk.

## Modes and Safety

Run `ws-mcp setup` to pick the mode and, by name, the accounts the agent may trade in.

`~/.ws-mcp/config.toml` picks the mode, and `config.toml` in this repo is the default it falls back to. A config that fails to parse stops ws-mcp instead of guessing.

- `read` (default): account data only. No order tools exist.
- `paper`: orders are simulated against live quotes, and positions, activity, and orders show the simulation.
- `trade`: orders reach the real account, only in `allowed_accounts` (empty by default) and within the limits.

Every order is previewed into a ticket, then placed with it once before it expires. Orders above `require_approval_above` also need `ws-mcp approve <ticket>`. `ws-mcp kill` halts all order actions until `ws-mcp resume`, and so does `WS_KILL=1`. Every decision is appended to `~/.ws-mcp/audit.jsonl`.

The session is in the system keychain and everything else is under `~/.ws-mcp`. `ws-mcp clear` removes the session, the audit log, tickets, budgets, and the paper ledger, and keeps your config and kill switch.
