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
copy is at <https://docs.saturation.io/openapi.yaml>. `src/v1/generated.rs` is
generated. Do not edit it by hand.

When the API contract changes, update the snapshot, regenerate the client, run
the checks above, and include both files in the same pull request.

## Authentication

`saturation login` uses the OAuth authority at <https://connect.saturation.io>.
It must continue to use OAuth discovery, dynamic public-client registration,
PKCE, and the existing Saturation workspace consent screen. Do not add a second
login service or embed a client secret in the binary.

Personal API tokens remain available through `saturation auth token` for CI and
headless use. Treat them as opaque values.

## Releases

The version in `Cargo.toml` must match the Git tag. A published GitHub release
builds the binaries, creates checksums and attestations, publishes the crate,
and updates `Saturation-IO/homebrew-tap`.

Release automation requires `CARGO_REGISTRY_TOKEN` and `HOMEBREW_TAP_TOKEN`.
