//! Build script for the Saturation CLI.
//!
//! Two responsibilities, both keyed off the vendored OpenAPI 3.1 at
//! `openapi/openapi.yaml` (a copy of `docs/next/next-api-build/openapi/openapi.yaml`,
//! the single source of truth that also drives the TS SDK, the Scalar docs and
//! the contract tests):
//!
//! 1. **`/v1` operation inventory** (active). Extract `{ path, method,
//!    operationId, summary }` for every operation into `$OUT_DIR/v1_operations.json`,
//!    which `src/schema.rs` embeds via `include_str!`. This keeps `sat schema`
//!    in sync with the spec with zero hand-maintenance and no runtime YAML
//!    dependency. The extraction is a deliberately small, dependency-free line
//!    scanner — the document's own 2-space path indentation is the contract.
//!
//! 2. **progenitor codegen** (documented, opt-in). The chosen generator for the
//!    typed `/v1` client is **progenitor** (pure-Rust, `reqwest`-native — no
//!    JVM/codegen-server, matching the crate's existing `reqwest 0.12` stack).
//!    Wiring it as a hard `build-dependency` pulls a large transitive tree
//!    (`typify`, `openapiv3`, `schemars`, a vendored `rustfmt`) and a multi-minute
//!    first build, so the committed `src/v1/generated.rs` provides a
//!    deterministic, offline client today. To regenerate against an updated spec,
//!    enable the documented step below (see README "Regenerating the client").

use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let spec_path = manifest_dir.join("openapi/openapi.yaml");
    println!("cargo:rerun-if-changed=openapi/openapi.yaml");
    println!("cargo:rerun-if-changed=build.rs");

    let spec = std::fs::read_to_string(&spec_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", spec_path.display()));

    let operations = extract_operations(&spec);
    let json = serde_operations_to_json(&operations);

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    std::fs::write(out_dir.join("v1_operations.json"), json)
        .expect("failed to write v1_operations.json");

    // ── progenitor codegen (opt-in via the `progenitor-gen` feature) ──────────
    //
    // Uncomment the build-dependency in Cargo.toml and the block below to run
    // progenitor at build time. It writes a generated client to
    // `$OUT_DIR/v1_client.rs`, which `src/v1/generated.rs` can then `include!`.
    //
    //   #[cfg(feature = "progenitor-gen")]
    //   {
    //       let raw = std::fs::read_to_string(&spec_path).unwrap();
    //       let spec: openapiv3::OpenAPI = serde_yaml::from_str(&raw).unwrap();
    //       let mut gen = progenitor::Generator::default();
    //       let tokens = gen.generate_tokens(&spec).unwrap();
    //       let ast = syn::parse2(tokens).unwrap();
    //       let content = prettyplease::unparse(&ast);
    //       std::fs::write(out_dir.join("v1_client.rs"), content).unwrap();
    //   }
}

#[derive(Clone)]
struct Operation {
    path: String,
    method: String,
    operation_id: Option<String>,
    summary: Option<String>,
}

/// Dependency-free extraction of operations from the OpenAPI YAML.
///
/// The `paths:` block uses a stable shape: 2-space-indented path keys, 4-space
/// HTTP-method keys, 6-space `operationId:` / `summary:` keys. We track the
/// current path + method and capture the first `operationId`/`summary` under
/// each method. This is intentionally narrow (the spec is machine-generated and
/// the indentation is part of its contract) and is covered by a schema.rs test
/// that asserts a known operation is present.
fn extract_operations(spec: &str) -> Vec<Operation> {
    let mut ops: Vec<Operation> = Vec::new();
    let mut in_paths = false;
    let mut current_path: Option<String> = None;
    let mut current: Option<Operation> = None;

    for line in spec.lines() {
        // Enter/exit the top-level `paths:` block (column 0).
        if line == "paths:" {
            in_paths = true;
            continue;
        }
        if in_paths && !line.is_empty() && !line.starts_with(' ') {
            // A new top-level key (e.g. `components:`) ends the paths block.
            if let Some(op) = current.take() {
                ops.push(op);
            }
            break;
        }
        if !in_paths {
            continue;
        }

        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim_start();

        // Path key: 2-space indent, starts with `/`, ends with `:`.
        if indent == 2 && trimmed.starts_with('/') && trimmed.ends_with(':') {
            if let Some(op) = current.take() {
                ops.push(op);
            }
            current_path = Some(trimmed.trim_end_matches(':').to_string());
            continue;
        }

        // Method key: 4-space indent, a known verb, ends with `:`.
        if indent == 4 && trimmed.ends_with(':') {
            let verb = trimmed.trim_end_matches(':');
            if is_http_method(verb) {
                if let Some(op) = current.take() {
                    ops.push(op);
                }
                if let Some(path) = &current_path {
                    current = Some(Operation {
                        path: path.clone(),
                        method: verb.to_uppercase(),
                        operation_id: None,
                        summary: None,
                    });
                }
                continue;
            }
        }

        // operationId / summary: 6-space indent under the current method.
        if indent == 6 {
            if let Some(op) = current.as_mut() {
                if let Some(rest) = trimmed.strip_prefix("operationId:") {
                    if op.operation_id.is_none() {
                        op.operation_id = Some(rest.trim().to_string());
                    }
                } else if let Some(rest) = trimmed.strip_prefix("summary:") {
                    if op.summary.is_none() {
                        let s = rest.trim().trim_matches('"').to_string();
                        if !s.is_empty() {
                            op.summary = Some(s);
                        }
                    }
                }
            }
        }
    }

    if let Some(op) = current.take() {
        ops.push(op);
    }
    ops
}

fn is_http_method(m: &str) -> bool {
    matches!(
        m,
        "get" | "post" | "put" | "patch" | "delete" | "head" | "options"
    )
}

/// Serialize without pulling serde into the build script.
fn serde_operations_to_json(ops: &[Operation]) -> String {
    let mut out = String::from("[");
    for (i, op) in ops.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"path\":");
        push_json_string(&mut out, &op.path);
        out.push_str(",\"method\":");
        push_json_string(&mut out, &op.method);
        out.push_str(",\"operationId\":");
        match &op.operation_id {
            Some(id) => push_json_string(&mut out, id),
            None => out.push_str("null"),
        }
        out.push_str(",\"summary\":");
        match &op.summary {
            Some(s) => push_json_string(&mut out, s),
            None => out.push_str("null"),
        }
        out.push('}');
    }
    out.push(']');
    out
}

fn push_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}
