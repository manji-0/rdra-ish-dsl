//! Shared table/CSV/JSON formatting helpers.

use anyhow::Result;

use crate::cli::ListFormat;

fn table_separator(col_widths: &[usize]) -> String {
    col_widths
        .iter()
        .map(|&w| "\u{2500}".repeat(w))
        .collect::<Vec<_>>()
        .join("  ")
}
pub(crate) fn bool_cell(value: bool) -> String {
    (if value { "true" } else { "false" }).to_string()
}

pub(crate) fn format_rows<const N: usize>(
    headers: &[&str; N],
    rows: &[[String; N]],
    format: &ListFormat,
    empty_label: &str,
) -> Result<String> {
    match format {
        ListFormat::Table => {
            if rows.is_empty() {
                return Ok(format!("No {} found.\n", empty_label));
            }
            let mut col_widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
            for row in rows {
                for (i, cell) in row.iter().enumerate() {
                    col_widths[i] = col_widths[i].max(cell.chars().count());
                }
            }
            let mut out = String::new();
            let header_line: Vec<String> = headers
                .iter()
                .enumerate()
                .map(|(i, h)| format!("{:<width$}", h.to_uppercase(), width = col_widths[i]))
                .collect();
            out.push_str(&header_line.join("  "));
            out.push('\n');
            let sep_line: Vec<String> = col_widths.iter().map(|&w| "\u{2500}".repeat(w)).collect();
            out.push_str(&sep_line.join("  "));
            out.push('\n');
            for row in rows {
                let row_line: Vec<String> = row
                    .iter()
                    .enumerate()
                    .map(|(i, cell)| format!("{:<width$}", cell, width = col_widths[i]))
                    .collect();
                out.push_str(&row_line.join("  "));
                out.push('\n');
            }
            Ok(out)
        }
        ListFormat::Csv => {
            let mut out = format!("{}\n", headers.join(","));
            for row in rows {
                let cells: Vec<String> = row.iter().map(|c| csv_field(c)).collect();
                out.push_str(&format!("{}\n", cells.join(",")));
            }
            Ok(out)
        }
        ListFormat::Json => {
            let entries: Vec<String> = rows
                .iter()
                .map(|row| {
                    let fields: Vec<String> = headers
                        .iter()
                        .enumerate()
                        .map(|(i, header)| {
                            format!(
                                "{}:{}",
                                serde_json::to_string(header).unwrap(),
                                serde_json::to_string(&row[i]).unwrap()
                            )
                        })
                        .collect();
                    format!("{{{}}}", fields.join(","))
                })
                .collect();
            Ok(format!("[{}]\n", entries.join(",")))
        }
    }
}

pub(crate) fn format_id_label(
    items: &[(&str, &str)],
    format: &ListFormat,
    empty_label: &str,
) -> Result<String> {
    match format {
        ListFormat::Table => {
            if items.is_empty() {
                return Ok(format!("No {} found.\n", empty_label));
            }
            let id_w = items
                .iter()
                .map(|(id, _)| id.len())
                .max()
                .unwrap_or(2)
                .max(2);
            let label_w = items
                .iter()
                .map(|(_, l)| l.chars().count())
                .max()
                .unwrap_or(5)
                .max(5);
            let header_id = format!("{:<width$}", "ID", width = id_w);
            let header_label = format!("{:<width$}", "LABEL", width = label_w);
            let sep_id = table_separator(&[id_w]);
            let sep_label = table_separator(&[label_w]);
            let mut out = format!(
                "{}  {}\n{}  {}\n",
                header_id, header_label, sep_id, sep_label
            );
            for (id, label) in items {
                out.push_str(&format!("{:<width$}  {}\n", id, label, width = id_w));
            }
            Ok(out)
        }
        ListFormat::Csv => {
            let mut out = String::from("id,label\n");
            for (id, label) in items {
                // Simple CSV: quote if contains comma or quote
                let escaped_id = csv_field(id);
                let escaped_label = csv_field(label);
                out.push_str(&format!("{},{}\n", escaped_id, escaped_label));
            }
            Ok(out)
        }
        ListFormat::Json => {
            let entries: Vec<String> = items
                .iter()
                .map(|(id, label)| {
                    format!(
                        "{{\"id\":{},\"label\":{}}}",
                        serde_json::to_string(id).unwrap(),
                        serde_json::to_string(label).unwrap()
                    )
                })
                .collect();
            Ok(format!("[{}]\n", entries.join(",")))
        }
    }
}

pub(crate) fn optional_cell(value: &Option<String>) -> String {
    value.clone().unwrap_or_default()
}

pub(crate) fn repeated_cell(values: &[String]) -> String {
    values.join("|")
}
pub(crate) fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
