//! `fmt` CLI command.

use std::path::PathBuf;

use anyhow::{Context, Result};
use rdra_ish_syntax::format_source;
use std::fs;

use crate::load::collect_rdra_files;

pub fn run_fmt(inputs: &[PathBuf], write: bool, check: bool) -> Result<()> {
    if write && check {
        anyhow::bail!("--write and --check cannot be combined");
    }

    let mut files = collect_rdra_files(inputs)?;
    files.sort();
    if files.is_empty() {
        anyhow::bail!("no .rdra files found in the given inputs");
    }

    let multiple_files = files.len() > 1;
    let mut changed = Vec::new();
    for (index, file) in files.into_iter().enumerate() {
        let src = fs::read_to_string(&file)
            .with_context(|| format!("failed to read {}", file.display()))?;
        let formatted = format_source(&src)
            .map_err(|err| anyhow::anyhow!("parse errors: {:?}", err.parse_errors))
            .with_context(|| format!("failed to format {}", file.display()))?;
        if formatted != src {
            changed.push(file.clone());
            if write {
                fs::write(&file, &formatted)
                    .with_context(|| format!("failed to write {}", file.display()))?;
            }
        }

        if !write && !check {
            if multiple_files {
                if index > 0 {
                    println!();
                }
                println!("// {}", file.display());
            }
            print!("{}", formatted);
        }
    }

    if check && !changed.is_empty() {
        for file in &changed {
            eprintln!("needs formatting: {}", file.display());
        }
        std::process::exit(1);
    }

    if write {
        println!("formatted {} file(s)", changed.len());
    } else if check {
        println!("OK: all files formatted");
    }

    Ok(())
}
