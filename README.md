# Saturation CLI

[![CI](https://github.com/Saturation-IO/saturation-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/Saturation-IO/saturation-cli/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Use the [Saturation API](https://docs.saturation.io) from a terminal, script, or agent.

The `saturation` command covers production finance resources such as projects,
budgets, transactions, purchase orders, documents, and workspace search. It also
exposes the agent tool registry for automation workflows. Commands return JSON by
default and can also return tables or CSV.

## Install

### Homebrew

```console
brew install Saturation-IO/tap/saturation
```

### macOS and Linux

```console
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/Saturation-IO/saturation-cli/releases/latest/download/saturation-cli-installer.sh | sh
```

### Windows

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/Saturation-IO/saturation-cli/releases/latest/download/saturation-cli-installer.ps1 | iex"
```

### Cargo

```console
cargo install saturation-cli --locked
```

Each method installs the `saturation` binary. GitHub releases include SHA-256
checksums and build provenance for each binary.

## Get started

Create a personal token in Saturation under **Settings > Developers > API**, then
save it locally:

```console
saturation auth token "$SATURATION_API_TOKEN"
```

Check the connection and list your projects:

```console
saturation auth status
saturation v1 projects list
```

The CLI stores credentials in `~/.saturation/config.json`. On Unix systems, the
directory is readable only by your user and the file uses mode `0600`.

For CI, pass a protected token file instead of saving credentials:

```console
saturation --token-file /run/secrets/saturation-token v1 projects list
```

## Examples

```console
# Show projects as a table
saturation --format table v1 projects list

# Search the workspace
saturation v1 search "camera rental"

# List transactions for a project
saturation v1 --project PROJECT_ID transactions list

# Discover every command and agent tool as JSON
saturation schema
```

Run `saturation --help` to see the full command tree. Run any command with
`--help` for its arguments and examples.

## Command groups

| Command | Purpose |
| --- | --- |
| `saturation auth` | Save, inspect, or remove an API token |
| `saturation v1` | Work with the public API resource grammar |
| `saturation agent` | Discover and invoke workspace agent tools |
| `saturation workspace` | Select the workspace used by agent tools |
| `saturation schema` | Print machine-readable command and tool definitions |

The `v1` client is generated from the same OpenAPI specification used by the
[TypeScript SDK](https://github.com/Saturation-IO/saturation-sdk-typescript) and
the [API documentation](https://docs.saturation.io).

## Configuration

| Option | Environment variable | Purpose |
| --- | --- | --- |
| `--token-file` | `SATURATION_TOKEN_FILE` | Read a bearer token from a file for each command |
| `--api-base-url` | `SATURATION_API_BASE_URL` | Override the public API base URL |
| `--server` | `SATURATION_SERVER_URL` | Override the auth and agent server URL |
| `--format` | | Choose `json`, `table`, or `csv` output |
| `--quiet` | | Print data without headers or metadata |

The token selects the workspace for public API commands. `saturation workspace`
controls the separate agent tool context.

## Build from source

The CLI requires Rust 1.85 or newer.

```console
git clone https://github.com/Saturation-IO/saturation-cli.git
cd saturation-cli
cargo build --locked
cargo test --locked
```

The development binary is written to `target/debug/saturation`.

## Verify a release

Download the archive and `sha256.sum` from the
[release page](https://github.com/Saturation-IO/saturation-cli/releases), then run:

```console
shasum -a 256 --check sha256.sum
```

GitHub also records an artifact attestation for each published binary.

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
