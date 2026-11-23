use super::{whole_job, Job, JobKind};
use crate::export::tla_file;
use anyhow::{Context, Result};
use rdra_ish_core::lifecycle::TransitionSystem;
use rdra_ish_emit::tla::{TlaBundle, TlaNames};
use std::fs;
use std::path::{Path, PathBuf};

/// The whole-model module name, and the directory `-o` names: `-o X.tla` writes next to
/// `X.tla` and names the whole-model module `X`; any other `-o` is a directory.
pub(super) fn out_names(out: Option<PathBuf>) -> (Option<PathBuf>, String) {
    match out {
        Some(path) => match tla_file(&path) {
            Some((parent, stem)) => (Some(parent), stem),
            None => (Some(path), "RdraSpec".to_string()),
        },
        None => (None, "RdraSpec".to_string()),
    }
}

/// Creates the directory the specs go to: `out`, or a temp dir when `-o` names none.
pub(super) fn spec_dir(out: Option<PathBuf>) -> Result<PathBuf> {
    let dir = out.unwrap_or_else(|| {
        std::env::temp_dir().join(format!("rdra-ish-tlc-{}", std::process::id()))
    });
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    // TLC runs inside `dir`, so the metadir and java tmpdir paths must not be relative.
    dunce::canonicalize(&dir).with_context(|| format!("failed to resolve {}", dir.display()))
}

fn write_spec(dir: &Path, module: &str, tla: &str, cfg: &str) -> Result<()> {
    for (ext, text) in [("tla", tla), ("cfg", cfg)] {
        let path = dir.join(format!("{module}.{ext}"));
        fs::write(&path, text).with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}

/// Writes every job's specs, the whole-model module, and `cones.txt` under `dir`. Returns the
/// index text and its path.
pub(super) fn write_specs_and_index<'a>(
    dir: &Path,
    jobs: impl IntoIterator<Item = &'a Job>,
    whole: &Option<TlaBundle>,
) -> Result<(String, PathBuf)> {
    let mut index = String::new();
    for job in jobs {
        write_spec(dir, &job.module, &job.tla, &job.cfg)?;
        index.push_str(&format!("{}.tla: {}\n", job.module, job.checks.join(" ")));
        if let JobKind::PropertyCone(p) = &job.kind {
            for extra in p.full.iter().chain(&p.wf) {
                write_spec(dir, &extra.module, &extra.tla, &extra.cfg)?;
                index.push_str(&format!("{}.tla: {}\n", extra.module, job.checks.join(" ")));
            }
        }
    }
    if let Some(w) = &whole {
        write_spec(dir, &w.module_name, &w.tla, &w.cfg)?;
    }
    let index_path = dir.join("cones.txt");
    fs::write(&index_path, &index)
        .with_context(|| format!("failed to write {}", index_path.display()))?;
    Ok((index, index_path))
}

/// Writes the whole-model module of each property in `open` that `cones.txt` does not list
/// yet, and the index with them.
pub(super) fn write_whole_specs(
    dir: &Path,
    (index, index_path): (&mut String, &Path),
    (ts, names): (&TransitionSystem, &TlaNames),
    whole_modules: &[String],
    open: &[usize],
) -> Result<()> {
    let wholes: Vec<Job> = open
        .iter()
        .filter(|&&i| {
            let listed = format!("{}.tla: ", whole_modules[i]);
            !index.lines().any(|l| l.starts_with(&listed))
        })
        .map(|&i| whole_job(ts, names, &whole_modules[i], i))
        .collect();
    for job in &wholes {
        write_spec(dir, &job.module, &job.tla, &job.cfg)?;
        index.push_str(&format!("{}.tla: {}\n", job.module, job.checks.join(" ")));
    }
    if !wholes.is_empty() {
        fs::write(index_path, &index)
            .with_context(|| format!("failed to write {}", index_path.display()))?;
    }
    Ok(())
}
