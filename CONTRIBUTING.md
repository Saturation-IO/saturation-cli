# Contributing

Thanks for helping improve the Saturation CLI.

## Development

Install Rust 1.85 or newer, clone the repository, and run:

```console
cargo build --locked
cargo test --locked
```

Before opening a pull request, run the same checks as CI:

```console
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo publish --locked --dry-run
```

Keep changes focused. Add a regression test when fixing a bug, and update the
README when a command or installation method changes.

## OpenAPI client

The repository includes a vendored OpenAPI specification at
`openapi/openapi.yaml`. The generated client at `src/v1/generated.rs` must be
updated in the same pull request when that specification changes.

## Security

Report suspected vulnerabilities through the private process in
[SECURITY.md](SECURITY.md).
