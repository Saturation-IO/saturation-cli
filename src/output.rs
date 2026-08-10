use crate::cli::OutputFormat;
use anyhow::Result;
use colored::Colorize;
use serde::Serialize;

pub struct Output {
    pub format: OutputFormat,
    pub quiet: bool,
}

impl Output {
    pub fn new(format: OutputFormat, quiet: bool) -> Self {
        Self { format, quiet }
    }

    pub fn print<T: Serialize>(&self, data: &T) -> Result<()> {
        match self.format {
            OutputFormat::Json => {
                let json = if self.quiet {
                    serde_json::to_string(data)?
                } else {
                    serde_json::to_string_pretty(data)?
                };
                println!("{json}");
            }
            OutputFormat::Table => {
                // For table output, serialize to JSON value and format as table
                let value = serde_json::to_value(data)?;
                print_table(&value, self.quiet);
            }
            OutputFormat::Csv => {
                let value = serde_json::to_value(data)?;
                print_csv(&value);
            }
        }
        Ok(())
    }

    pub fn success(&self, message: &str) {
        if !self.quiet {
            println!("{} {message}", "OK".green().bold());
        }
    }

    pub fn error(&self, message: &str) {
        eprintln!("{} {message}", "ERROR".red().bold());
    }

    pub fn status(&self, label: &str, value: &str) {
        if !self.quiet {
            println!("{}: {value}", label.bold());
        }
    }
}

fn print_table(value: &serde_json::Value, quiet: bool) {
    match value {
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                if !quiet {
                    println!("(no results)");
                }
                return;
            }

            // Collect all keys from first item for column headers
            let headers: Vec<String> = if let Some(serde_json::Value::Object(first)) = items.first()
            {
                first.keys().cloned().collect()
            } else {
                // Not objects — print each value on its own line
                for item in items {
                    println!("{}", format_cell(item));
                }
                return;
            };

            // Calculate column widths
            let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
            let rows: Vec<Vec<String>> = items
                .iter()
                .filter_map(|item| {
                    item.as_object().map(|obj| {
                        headers
                            .iter()
                            .enumerate()
                            .map(|(i, key)| {
                                let cell =
                                    format_cell(obj.get(key).unwrap_or(&serde_json::Value::Null));
                                if cell.len() > widths[i] {
                                    widths[i] = cell.len();
                                }
                                cell
                            })
                            .collect()
                    })
                })
                .collect();

            // Cap column widths at 40 chars
            for w in &mut widths {
                if *w > 40 {
                    *w = 40;
                }
            }

            // Print header
            if !quiet {
                let header: String = headers
                    .iter()
                    .enumerate()
                    .map(|(i, h)| format!("{:<width$}", h, width = widths[i]))
                    .collect::<Vec<_>>()
                    .join("  ");
                println!("{}", header.bold());
                let separator: String = widths
                    .iter()
                    .map(|w| "-".repeat(*w))
                    .collect::<Vec<_>>()
                    .join("  ");
                println!("{separator}");
            }

            // Print rows
            for row in &rows {
                let line: String = row
                    .iter()
                    .enumerate()
                    .map(|(i, cell)| {
                        let truncated = if cell.len() > widths[i] {
                            format!("{}...", &cell[..widths[i].saturating_sub(3)])
                        } else {
                            cell.clone()
                        };
                        format!("{:<width$}", truncated, width = widths[i])
                    })
                    .collect::<Vec<_>>()
                    .join("  ");
                println!("{line}");
            }
        }
        serde_json::Value::Object(obj) => {
            for (key, val) in obj {
                println!("{}: {}", key.bold(), format_cell(val));
            }
        }
        other => {
            println!("{}", format_cell(other));
        }
    }
}

fn print_csv(value: &serde_json::Value) {
    if let serde_json::Value::Array(items) = value {
        if items.is_empty() {
            return;
        }

        // Headers from first item
        if let Some(serde_json::Value::Object(first)) = items.first() {
            let headers: Vec<&String> = first.keys().collect();
            println!(
                "{}",
                headers
                    .iter()
                    .map(|h| csv_escape(h))
                    .collect::<Vec<_>>()
                    .join(",")
            );

            for item in items {
                if let serde_json::Value::Object(obj) = item {
                    let row: Vec<String> = headers
                        .iter()
                        .map(|key| {
                            csv_escape(&format_cell(
                                obj.get(*key).unwrap_or(&serde_json::Value::Null),
                            ))
                        })
                        .collect();
                    println!("{}", row.join(","));
                }
            }
        }
    }
}

fn format_cell(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_cell_types() {
        assert_eq!(format_cell(&serde_json::Value::Null), "");
        assert_eq!(format_cell(&serde_json::json!(true)), "true");
        assert_eq!(format_cell(&serde_json::json!(42)), "42");
        assert_eq!(format_cell(&serde_json::json!("hello")), "hello");
        assert_eq!(format_cell(&serde_json::json!({"a": 1})), "{\"a\":1}");
    }

    #[test]
    fn test_csv_escape() {
        assert_eq!(csv_escape("simple"), "simple");
        assert_eq!(csv_escape("has,comma"), "\"has,comma\"");
        assert_eq!(csv_escape("has\"quote"), "\"has\"\"quote\"");
        assert_eq!(csv_escape("has\nnewline"), "\"has\nnewline\"");
    }

    #[test]
    fn test_output_json() {
        let output = Output::new(OutputFormat::Json, false);
        // Just verify it doesn't panic
        let data = serde_json::json!({"id": "test", "name": "example"});
        output.print(&data).unwrap();
    }

    #[test]
    fn test_output_json_quiet() {
        let output = Output::new(OutputFormat::Json, true);
        let data = serde_json::json!({"id": "test"});
        output.print(&data).unwrap();
    }
}
