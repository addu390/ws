# ws

Unofficial Wealthsimple MCP server in Rust. Not affiliated with or endorsed by Wealthsimple.

Wealthsimple has no public API. This talks to the same GraphQL API as the web app, which can change without notice, and automated access may conflict with Wealthsimple's terms. Use at your own risk.

## Crates

| Crate | Responsibility |
|---|---|
| `ws-core` | Domain types (money, ids, orders, positions, quotes). No IO. |
| `ws-config` | Typed loading of `config.toml` (mode, limits). |
| `ws-net` | Browser-emulating HTTP client and base headers. |
| `ws-auth` | Device bootstrap, login with 2FA, token refresh, session storage. |
| `ws-api` | Wealthsimple GraphQL operations and wire-to-domain conversion. |
| `ws-broker` | `Reader` and `Broker` traits, and the live Wealthsimple adapter. |
| `ws-paper` | Simulated broker for paper trading. |
| `ws-audit` | Append-only journal of every decision. |
| `ws-policy` | Permission levels, limits, budgets, kill switch. The only path to placing an order. |
| `ws-mcp` | MCP server binary and CLI. |

## Conventions

- Crates are named `ws-<word>`; source files are `<word>.rs`. No `mod.rs`, `utils.rs` or `helpers.rs`.
- Types with invariants have private fields and checked constructors.
- Each crate owns its `error.rs`.

## License

Apache-2.0. Protocol details are informed by [gboudreau/ws-api-python](https://github.com/gboudreau/ws-api-python) (GPL-3.0); porting its code directly would require relicensing this project as GPL-3.0.
