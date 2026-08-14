# currency-mcp

A [Model Context Protocol](https://modelcontextprotocol.io/) server written in Rust. It exposes live and historical **world currency exchange** tools plus prompts that steer an assistant to fetch rates before answering.

Rates come from [Frankfurter](https://frankfurter.dev/) (`api.frankfurter.dev`): reference data from dozens of central banks, 200+ currencies, no API key. These are **not** live tradable quotes from a bank or FX booth.

## Tools

| Tool | What it does |
| --- | --- |
| `list_currencies` | List ISO codes, names, and symbols. Optional `query` filter. |
| `get_currency` | Details and provider coverage for one code. |
| `get_latest_rates` | Latest rates for a `base` (default `USD`), optional `quotes`. |
| `get_rate` | One pair (`from` → `to`), optional historical `date`. |
| `convert_currency` | Convert `amount` between two currencies. |
| `get_historical_rates` | All or selected quotes on a given `date`. |
| `get_rate_history` | Daily series between `from_date` and `to_date`. Optional `group` of `week` or `month`. |

## Prompts

| Prompt | What it sets up |
| --- | --- |
| `convert_money` | Convert an amount and explain the rate. |
| `travel_budget` | Turn a home-currency budget into destination-currency guidance. |
| `compare_currencies` | Compare several quotes against a base. |
| `fx_briefing` | Short world FX briefing. |
| `historical_move` | Explain how a pair moved between two dates. |

## Build

```bash
cargo build --release
```

The binary is `target/release/currency-mcp`. It speaks MCP over **stdio** (logs go to stderr).

```bash
cargo test
```

## Use with Cursor

Add this to your MCP config (Cursor Settings → MCP), using the absolute path to the release binary:

```json
{
  "mcpServers": {
    "currency": {
      "command": "/Users/lowks/Projects/currency-mcp/target/release/currency-mcp"
    }
  }
}
```

Or run from the crate without installing:

```json
{
  "mcpServers": {
    "currency": {
      "command": "cargo",
      "args": ["run", "--quiet", "--release"],
      "cwd": "/Users/lowks/Projects/currency-mcp"
    }
  }
}
```

Then try a prompt such as “Convert 250 USD to MYR” or use the `convert_money` / `fx_briefing` prompts from the MCP prompts list.

## Inspector

```bash
npx @modelcontextprotocol/inspector cargo run --quiet
```
