use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::NamedTempFile;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn binary() -> PathBuf {
    assert_cmd::cargo::cargo_bin("saturation")
}

fn help(path: &[String]) -> String {
    let output = Command::new(binary())
        .args(path)
        .arg("--help")
        .output()
        .expect("run CLI help");
    assert!(
        output.status.success(),
        "help failed for {}",
        path.join(" ")
    );
    String::from_utf8(output.stdout).expect("help must be UTF-8")
}

fn child_commands(help: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut in_commands = false;
    for line in help.lines() {
        if line == "Commands:" {
            in_commands = true;
            continue;
        }
        if !in_commands {
            continue;
        }
        if line.is_empty() {
            break;
        }
        let Some(rest) = line.strip_prefix("  ") else {
            break;
        };
        if rest.starts_with(char::is_whitespace) {
            continue;
        }
        if let Some(name) = rest.split_whitespace().next() {
            if name != "help" {
                commands.push(name.to_string());
            }
        }
    }
    commands
}

fn collect_leaves(path: Vec<String>, leaves: &mut Vec<Vec<String>>) {
    let children = child_commands(&help(&path));
    if children.is_empty() {
        leaves.push(path);
        return;
    }
    for child in children {
        let mut next = path.clone();
        next.push(child);
        collect_leaves(next, leaves);
    }
}

fn placeholder(name: &str, upload: &Path) -> String {
    match name {
        "DATA" => "{}".to_string(),
        "QUERY" => "parity-audit".to_string(),
        "FILE" => upload.display().to_string(),
        "KIND" => "transaction".to_string(),
        "ACCOUNT" => "1000".to_string(),
        "COLUMN" => "estimate".to_string(),
        other => format!("audit-{}", other.to_ascii_lowercase().replace('_', "-")),
    }
}

fn invocation(path: &[String], upload: &Path) -> Vec<String> {
    let help = help(path);
    let usage = help
        .lines()
        .find_map(|line| line.strip_prefix("Usage: saturation "))
        .expect("leaf help must include usage");
    usage
        .split_whitespace()
        .filter_map(|token| {
            if token == "[OPTIONS]" || (token.starts_with('[') && token.ends_with(']')) {
                return None;
            }
            if token.starts_with('<') && token.ends_with('>') {
                return Some(placeholder(&token[1..token.len() - 1], upload));
            }
            Some(token.to_string())
        })
        .collect()
}

fn template_matches(template: &str, concrete: &str) -> bool {
    let template = template.split('/').filter(|part| !part.is_empty());
    let concrete = concrete.split('/').filter(|part| !part.is_empty());
    let template = template.collect::<Vec<_>>();
    let concrete = concrete.collect::<Vec<_>>();
    template.len() == concrete.len()
        && template.iter().zip(concrete).all(|(expected, actual)| {
            (expected.starts_with('{') && expected.ends_with('}')) || *expected == actual
        })
}

fn run_api(
    server: &MockServer,
    token: &NamedTempFile,
    args: &[&str],
    idempotency_key: bool,
    project: bool,
) -> std::process::Output {
    let mut command = Command::new(binary());
    command.args([
        "--api-base-url",
        &server.uri(),
        "--token-file",
        token.path().to_str().unwrap(),
        "--quiet",
    ]);
    if project {
        command.args(["--project", "audit-project"]);
    }
    if idempotency_key {
        command.args(["--idempotency-key", "parity-audit-key"]);
    }
    command.args(args).output().expect("run API command")
}

#[tokio::test(flavor = "multi_thread")]
async fn exact_cli_operation_parity_matches_openapi_contract() {
    let schema_output = Command::new(binary())
        .arg("schema")
        .output()
        .expect("render schema");
    assert!(schema_output.status.success());
    let schema: Value = serde_json::from_slice(&schema_output.stdout).expect("valid schema JSON");
    let operations = schema["api"]["operations"]
        .as_array()
        .expect("operation inventory");
    assert_eq!(operations.len(), 154, "OpenAPI operation count changed");
    assert!(
        operations.iter().all(|operation| !operation["path"]
            .as_str()
            .is_some_and(|path| path == "/usage" || path.contains("/usage/"))),
        "usage operations must stay outside the public contract"
    );

    let top = child_commands(&help(&[]));
    let api_roots = top
        .into_iter()
        .filter(|name| !matches!(name.as_str(), "login" | "logout" | "schema"));
    let mut leaves = Vec::new();
    for root in api_roots {
        collect_leaves(vec![root], &mut leaves);
    }
    assert_eq!(leaves.len(), 154, "root API leaf count changed");

    let variants = leaves
        .iter()
        .map(|leaf| {
            let label = leaf.join(" ");
            let project = leaf.first().map(String::as_str) == Some("budget")
                || label.starts_with("library project ")
                || label == "search";
            (leaf.clone(), project)
        })
        .collect::<Vec<_>>();
    assert_eq!(variants.len(), 154);

    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;

    let mut token = NamedTempFile::new().expect("token file");
    token.write_all(b"parity-audit-token\n").unwrap();
    let mut upload = NamedTempFile::new().expect("upload file");
    upload.write_all(b"parity audit fixture\n").unwrap();

    for (leaf, project) in &variants {
        let mut args = vec![
            "--api-base-url".to_string(),
            server.uri(),
            "--token-file".to_string(),
            token.path().display().to_string(),
            "--idempotency-key".to_string(),
            "parity-audit-key".to_string(),
            "--quiet".to_string(),
        ];
        if *project {
            args.extend(["--project".to_string(), "audit-project".to_string()]);
        }
        args.extend(invocation(leaf, upload.path()));
        let output = Command::new(binary())
            .args(&args)
            .output()
            .expect("run leaf");
        assert!(
            output.status.success(),
            "{} failed: {}",
            leaf.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let requests = server.received_requests().await.expect("recorded requests");
    assert_eq!(requests.len(), 154);
    let mut reached = HashSet::new();
    for ((leaf, project), request) in variants.iter().zip(&requests) {
        let concrete = request.url.path().trim_start_matches("/v1");
        let method = request.method.as_str();
        let mut matches = operations
            .iter()
            .filter(|operation| {
                operation["method"].as_str() == Some(method)
                    && operation["path"]
                        .as_str()
                        .is_some_and(|path| template_matches(path, concrete))
            })
            .collect::<Vec<_>>();
        let specificity = matches
            .iter()
            .map(|operation| {
                operation["path"]
                    .as_str()
                    .unwrap()
                    .split('/')
                    .filter(|part| !part.is_empty() && !part.starts_with('{'))
                    .count()
            })
            .max()
            .unwrap_or_else(|| {
                panic!(
                    "{} project={project} has no OpenAPI match for {method} {concrete}",
                    leaf.join(" ")
                )
            });
        matches.retain(|operation| {
            operation["path"]
                .as_str()
                .unwrap()
                .split('/')
                .filter(|part| !part.is_empty() && !part.starts_with('{'))
                .count()
                == specificity
        });
        assert_eq!(
            matches.len(),
            1,
            "{} project={project} matched {} operations for {method} {concrete}",
            leaf.join(" "),
            matches.len()
        );
        reached.insert(matches[0]["operationId"].as_str().unwrap().to_string());
    }
    assert_eq!(
        reached.len(),
        154,
        "two CLI variants reached the same operation"
    );

    let uncovered = operations
        .iter()
        .filter(|operation| {
            !reached.contains(operation["operationId"].as_str().expect("operation id"))
        })
        .map(|operation| {
            (
                operation["method"].as_str().unwrap(),
                operation["path"].as_str().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(uncovered, Vec::<(&str, &str)>::new());
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_queries_and_conditional_headers_reach_the_wire() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;
    let mut token = NamedTempFile::new().unwrap();
    token.write_all(b"test-token\n").unwrap();

    let cases: &[&[&str]] = &[
        &[
            "budget",
            "get",
            "--account-id",
            "account-1",
            "--tags",
            "tag-1,tag-2",
            "--tag-mode",
            "all",
            "--date-from",
            "2026-01-01",
            "--date-to",
            "2026-01-31",
            "--include-hidden-phases",
            "--expand",
            "lines",
            "--if-none-match",
            "budget-etag",
        ],
        &["budget", "phase-totals", "--if-none-match", "budget-etag"],
        &["budget", "lines", "delete", "line-1", "--reset"],
        &["purchase-orders", "create", "{}", "--expand", "contact"],
        &[
            "purchase-orders",
            "update",
            "po-1",
            "{}",
            "--expand",
            "contact",
        ],
        &[
            "library", "project", "fringes", "add", "source-1", "--reset",
        ],
        &[
            "library", "project", "globals", "add", "source-1", "--reset",
        ],
        &[
            "library",
            "project",
            "currencies",
            "add",
            "source-1",
            "--reset",
        ],
        &[
            "library",
            "project",
            "fringe-groups",
            "add",
            "source-1",
            "--reset",
        ],
    ];
    for args in cases {
        let output = run_api(&server, &token, args, true, true);
        assert!(
            output.status.success(),
            "{} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), cases.len());
    for request in &requests[..2] {
        assert_eq!(
            request
                .headers
                .get("if-none-match")
                .and_then(|value| value.to_str().ok()),
            Some("budget-etag")
        );
    }
    let query = |index: usize| {
        requests[index]
            .url
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect::<std::collections::HashMap<_, _>>()
    };
    let document = query(0);
    for (key, value) in [
        ("accountId", "account-1"),
        ("tags", "tag-1,tag-2"),
        ("tagMode", "all"),
        ("dateFrom", "2026-01-01"),
        ("dateTo", "2026-01-31"),
        ("includeHiddenPhases", "true"),
        ("expand", "lines"),
    ] {
        assert_eq!(document.get(key).map(String::as_str), Some(value));
    }
    assert_eq!(query(2).get("reset").map(String::as_str), Some("true"));
    assert_eq!(query(3).get("expand").map(String::as_str), Some("contact"));
    assert_eq!(query(4).get("expand").map(String::as_str), Some("contact"));
    for index in 5..9 {
        assert_eq!(query(index).get("reset").map(String::as_str), Some("true"));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn document_upload_and_content_follow_the_openapi_media_types() {
    let upload_server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&upload_server)
        .await;
    let mut token = NamedTempFile::new().unwrap();
    token.write_all(b"test-token\n").unwrap();
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(b"document bytes").unwrap();

    let output = run_api(
        &upload_server,
        &token,
        &[
            "documents",
            "upload",
            file.path().to_str().unwrap(),
            "--name",
            "invoice.pdf",
            "--description",
            "Vendor invoice",
            "--folder-id",
            "folder-1",
        ],
        true,
        false,
    );
    assert!(output.status.success());
    let requests = upload_server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body = String::from_utf8_lossy(&requests[0].body);
    for expected in ["invoice.pdf", "Vendor invoice", "folderId", "folder-1"] {
        assert!(body.contains(expected), "multipart body missing {expected}");
    }
    assert!(!body.contains("classification"));
    let upload_help = help(&["documents".to_string(), "upload".to_string()]);
    assert!(!upload_help.contains("--classification"));

    let content_server = MockServer::start().await;
    Mock::given(any())
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/octet-stream")
                .set_body_bytes(b"raw document\0bytes"),
        )
        .mount(&content_server)
        .await;
    let output = run_api(
        &content_server,
        &token,
        &["documents", "content", "document-1"],
        false,
        false,
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"raw document\0bytes");
}

#[tokio::test(flavor = "multi_thread")]
async fn every_required_create_rejects_a_missing_idempotency_key() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;
    let mut token = NamedTempFile::new().unwrap();
    token.write_all(b"test-token\n").unwrap();
    let required: &[&[&str]] = &[
        &["projects", "create", "{}"],
        &["spaces", "create", "{}"],
        &["contacts", "create", "{}"],
        &["budget", "lines", "create", "{}"],
        &["budget", "lines", "bulk", "{}"],
        &["budget", "phase-data", "bulk", "{}"],
        &["budget", "phases", "create", "{}"],
        &["transactions", "create", "{}"],
        &["transactions", "bulk", "{}"],
        &["transactions", "items", "tx-1", "create", "{}"],
        &["purchase-orders", "create", "{}"],
        &["purchase-orders", "items", "po-1", "create", "{}"],
        &["library", "rate-packs", "create", "{}"],
        &["library", "rate-packs", "items", "pack-1", "create", "{}"],
        &["library", "fringes", "create", "{}"],
        &["library", "globals", "create", "{}"],
        &["library", "currencies", "create", "{}"],
        &["library", "fringe-groups", "create", "{}"],
        &["library", "tags", "create", "{}"],
        &["library", "units", "create", "{}"],
        &["projects", "comments", "project-1", "create", "{}"],
    ];
    assert_eq!(required.len(), 21);
    for args in required {
        let project = matches!(args.first(), Some(&"budget"));
        let output = run_api(&server, &token, args, false, project);
        assert!(
            !output.status.success(),
            "{} accepted no key",
            args.join(" ")
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("requires --idempotency-key"),
            "{} returned the wrong error: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn project_scope_is_required_only_for_project_owned_tasks() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;
    let mut token = NamedTempFile::new().unwrap();
    token.write_all(b"test-token\n").unwrap();

    let missing = run_api(&server, &token, &["budget", "get"], false, false);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("pass --project"));

    let unsupported = run_api(&server, &token, &["projects", "list"], false, true);
    assert!(!unsupported.status.success());
    assert!(String::from_utf8_lossy(&unsupported.stderr).contains("--project is not valid"));

    assert!(server.received_requests().await.unwrap().is_empty());
}
