# Contributor guide for coding agents

This repository publishes the `saturation` CLI. Keep its public commands,
documentation, and release artifacts aligned with the Saturation API.

## Checks

Run these before opening a pull request:

```console
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

## API contract

The public contract is [openapi.yaml](openapi/openapi.yaml). The canonical hosted
copy is at <https://docs.saturation.io/openapi.yaml>. The committed transport is
in `src/v1/generated.rs`.

When the API contract changes, update the snapshot and transport together, then
run the checks above.

## Authentication

`saturation login` uses the OAuth authority at <https://connect.saturation.io>.
It must continue to use OAuth discovery, dynamic public-client registration,
PKCE, and the existing Saturation workspace consent screen. Do not add a second
login service or embed a client secret in the binary.

Personal API tokens remain available for CI and headless use. Pass one through a
protected file with `--token-file` or `SATURATION_TOKEN_FILE`. Treat tokens as
opaque values.

## Releases

The version in `Cargo.toml` must match the Git tag. A version tag builds the
binaries. After every artifact passes, the release publishes the crate and
updates `Saturation-IO/homebrew-tap`.

Release jobs run through the protected `release` environment. The first crate
publish uses `CARGO_REGISTRY_TOKEN`; later releases use crates.io trusted
publishing. Homebrew publishing requires `HOMEBREW_TAP_TOKEN`.
