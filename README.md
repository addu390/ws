# WS MCP

Unofficial Wealthsimple MCP server in Rust. Not affiliated with or endorsed by Wealthsimple.

Wealthsimple has no public API. This talks to the same GraphQL API as the web app, which can change without notice, and automated access may conflict with Wealthsimple's terms. Use at your own risk.

## Install

```sh
git clone https://github.com/addu390/ws.git
cd ws
cargo install --path crates/ws-mcp
```

## Setup

```sh
ws-mcp login
ws-mcp setup
```

`ws-mcp login` is optional, since `setup` asks you to log in if needed.

This logs you in, picks the mode (`read`, `paper` or `trade`), and the accounts the agent may trade in. Then add it to your MCP client:

```json
{
  "mcpServers": {
    "ws-mcp": { "command": "ws-mcp" }
  }
}
```

Later, `ws-mcp logout` removes the session, and `ws-mcp clear` also removes the audit log, tickets, budgets, and the paper ledger.

## Example

Ask your agent "buy $50 of XEQT in my TFSA", or do it yourself:

```sh
ws-mcp preview-order tfsa XEQT buy --amount 50
ws-mcp place-order <ticket>
```

Run `ws-mcp --help` for everything else.

## Disclaimer

This software is provided as is, without warranty of any kind. It is not financial advice. Wealthsimple can change its API at any time, which may break this tool or cause orders to behave unexpectedly. You are solely responsible for every order placed through it and for any losses, account restrictions, or other consequences. The authors accept no liability.
