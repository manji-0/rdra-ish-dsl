//! Markdown tables, for pasting a list or a matrix into a design doc or a PRD.

use crate::EmitError;

/// A GitHub-flavoured Markdown table. A `|` in a cell is escaped and a line break becomes
/// `<br>`, so every row stays on one line.
pub fn table<H: AsRef<str>>(headers: &[H], rows: &[Vec<String>]) -> String {
    let line = |cells: &mut dyn Iterator<Item = &str>| {
        let cells: Vec<String> = cells.map(cell).collect();
        format!("| {} |\n", cells.join(" | "))
    };
    let mut out = line(&mut headers.iter().map(AsRef::as_ref));
    out.push_str(&format!("|{}\n", " --- |".repeat(headers.len())));
    for row in rows {
        out.push_str(&line(&mut row.iter().map(String::as_str)));
    }
    out
}

/// The CSV text an emitter wrote, header row first, as a Markdown table.
pub fn from_csv(csv: &str) -> Result<String, EmitError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(csv.as_bytes());
    let mut records = reader.records();
    let Some(headers) = records.next().transpose()? else {
        return Ok(String::new());
    };
    let headers: Vec<String> = headers.iter().map(str::to_string).collect();
    let rows = records
        .map(|r| r.map(|r| r.iter().map(str::to_string).collect()))
        .collect::<Result<Vec<Vec<String>>, _>>()?;
    Ok(table(&headers, &rows))
}

fn cell(text: &str) -> String {
    text.replace('|', "\\|")
        .replace("\r\n", "<br>")
        .replace('\n', "<br>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pipe_and_a_line_break_stay_inside_their_cell() {
        let md = table(&["id", "note"], &[vec!["A".into(), "x | y\nz".into()]]);
        assert_eq!(md, "| id | note |\n| --- | --- |\n| A | x \\| y<br>z |\n");
    }

    #[test]
    fn a_csv_with_quoted_commas_keeps_its_columns() {
        let md = from_csv("Api,Order\nPlace,\"C,R\"\n").unwrap();
        assert_eq!(md, "| Api | Order |\n| --- | --- |\n| Place | C,R |\n");
    }
}
