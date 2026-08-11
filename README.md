# Saturation CLI

[![CI](https://github.com/Saturation-IO/saturation-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/Saturation-IO/saturation-cli/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Use the [Saturation API](https://docs.saturation.io) from a terminal or script.

The `saturation` command covers projects, budgets, transactions, purchase orders,
documents, and workspace search. Commands return JSON by default and can also
return tables or CSV. API versioning stays behind the command interface.

## Install

macOS and Linux:

```console
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/Saturation-IO/saturation-cli/releases/latest/download/saturation-cli-installer.sh | sh
```

Windows PowerShell:

```powershell
irm https://github.com/Saturation-IO/saturation-cli/releases/latest/download/saturation-cli-installer.ps1 | iex
```

Homebrew:

```console
brew install Saturation-IO/tap/saturation
```

## Get started

Sign in through Saturation:

```console
saturation login
```

The CLI opens your browser for Saturation login, MFA, workspace selection, and
consent. The browser returns authorization to the local CLI through a short-lived
callback on `127.0.0.1`.

Check the session and list your projects:

```console
saturation whoami
saturation projects list
```

The CLI stores the OAuth session in `~/.saturation/config.json`. On Unix systems, the
directory is readable only by your user and the file uses mode `0600`.

For CI or another headless environment, create a personal token under
**Settings > Developers > API** and pass it through a protected file:

```console
saturation --token-file /run/secrets/saturation-token projects list
```

## Examples

```console
# Show projects as a table
saturation --format table projects list

# Search the workspace
saturation search "camera rental"

# List transactions for a project
saturation --project PROJECT_ID transactions list

# Create operations require an idempotency key
saturation --idempotency-key UNIQUE_KEY projects create '{"name":"Summer Feature"}'

# Upload and link a document
saturation --idempotency-key UNIQUE_KEY documents upload invoice.pdf
saturation documents link DOCUMENT_ID transaction TRANSACTION_ID

# Write document content to a file
saturation documents content DOCUMENT_ID > document.pdf

# Inspect the bundled public API contract
saturation schema
```

`documents content` writes the original bytes to standard output. Redirect the
command to a file or pipe it to another program.

Create operations require a unique `--idempotency-key`. Reusing a key with the
same request is safe. Reusing it with a different request returns a conflict.

Run `saturation --help` to see the full command tree. Run any command with
`--help` for its arguments and examples.

## Command groups

| Command | Purpose |
| --- | --- |
| `saturation login` | Sign in through Saturation OAuth |
| `saturation logout` | Clear the stored OAuth session |
| `saturation whoami` | Show the current identity and workspace |
| `saturation projects` | Work with projects and their comments |
| `saturation spaces` | Work with workspace spaces |
| `saturation contacts` | Work with workspace contacts |
| `saturation budget` | Work with a project's budget, lines, phases, and totals |
| `saturation search` | Search the current workspace or project |
| `saturation transactions` | Work with the production ledger |
| `saturation purchase-orders` | Work with purchase orders and their lifecycle |
| `saturation payment-requests` | Read requests for payment |
| `saturation payments` | Read payments and their timeline |
| `saturation library` | Work with workspace and project Library resources |
| `saturation documents` | Upload, read, and link documents |
| `saturation webhooks` | Manage subscriptions and inspect deliveries |
| `saturation schema` | Print the machine-readable public API inventory |

The repository commits its Rust transport and an audited snapshot of the public
OpenAPI contract. The snapshot contains 157 operations, and `saturation schema`
reads it directly. CI executes 157 CLI tasks against a local recorder and maps
each request to one OpenAPI operation. Every public operation has one CLI task.

The [API documentation](https://docs.saturation.io) uses the same public contract.
Machine-readable references are available in
[llms.txt](https://docs.saturation.io/llms.txt) and the
[OpenAPI specification](https://docs.saturation.io/openapi.yaml).

## Configuration

| Option | Environment variable | Purpose |
| --- | --- | --- |
| `--token-file` | `SATURATION_TOKEN_FILE` | Read a bearer token from a file for each command |
| `--api-base-url` | `SATURATION_API_BASE_URL` | Override the public API base URL |
| `--idempotency-key` | | Supply the unique key required by create operations |
| `--format` | | Choose `json`, `table`, or `csv` output |
| `--quiet` | | Print data without headers or metadata |

The token selects the workspace. Run `saturation whoami` to inspect the active
identity and workspace.

## Develop

Build and install from source with Rust 1.85 or newer:

```console
git clone https://github.com/Saturation-IO/saturation-cli.git
cd saturation-cli
cargo install --path . --locked
```

Run the development checks:

```console
cargo build --locked
cargo test --locked
```

The development binary is written to `target/debug/saturation`.

## Contributing

Issues and pull requests are welcome. Before opening a pull request, run:

```console
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

Security reports should follow [SECURITY.md](SECURITY.md).

## License

Saturation CLI is licensed under the [MIT License](LICENSE).
