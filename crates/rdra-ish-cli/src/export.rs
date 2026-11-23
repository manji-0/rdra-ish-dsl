//! CLI export artifact dispatch.

use std::collections::BTreeSet;
use std::fs;
use std::num::NonZeroU8;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use rdra_ish_core::lifecycle::{
    has_errors, Bounds, Built, Caps, Cone, EntId, Scope, Severity, Target, TransitionSystem,
};
use rdra_ish_core::SemanticModel;
use rdra_ish_emit::{
    asyncapi::AsyncApiJsonEmitter,
    dbml::DbmlEmitter,
    json_schema::JsonSchemaEmitter,
    mermaid::ErMermaidEmitter,
    openapi::OpenApiJsonEmitter,
    sql::SqlEmitter,
    tla::{self, PrintMode, TlaBundle},
    View,
};

use crate::cli::{ExportKind, SqlDialect};
use crate::lifecycle_gate::{gate, Lifecycle, Output, Reads};
use crate::load::load_clean;
use crate::memory::generator_caps;
use crate::state_findings::Finding;

/// What an export kind writes: an artifact its emitter prints, or the TLA+ bundle, which is
/// built from the transition system and is not an [`Emitter`].
pub enum Export {
    Artifact(Output, &'static str),
    Tla,
}

pub fn export_output(kind: &ExportKind, dialect: SqlDialect) -> Export {
    match kind {
        ExportKind::Openapi => {
            Export::Artifact(Output::declared(OpenApiJsonEmitter), "openapi.json")
        }
        ExportKind::Asyncapi => {
            Export::Artifact(Output::declared(AsyncApiJsonEmitter), "asyncapi.json")
        }
        ExportKind::Dbml => Export::Artifact(Output::declared(DbmlEmitter), "schema.dbml"),
        ExportKind::Sql => Export::Artifact(
            Output::declared(SqlEmitter {
                dialect: dialect.dialect(),
            }),
            "schema.sql",
        ),
        ExportKind::JsonSchema => {
            Export::Artifact(Output::declared(JsonSchemaEmitter), "json-schema.json")
        }
        ExportKind::MermaidEr => Export::Artifact(Output::declared(ErMermaidEmitter), "er.mmd"),
        ExportKind::Tla => Export::Tla,
    }
}

pub(crate) fn run_export(
    inputs: &[PathBuf],
    kind: &ExportKind,
    out: &Path,
    instance_count: u32,
    int_range_max: u32,
    dialect: SqlDialect,
) -> Result<()> {
    let (program, model, manifest) = load_clean(inputs)?;
    let caps = generator_caps(manifest.as_ref());
    // An artifact gates by what its emitter reads. The TLA+ export gates after `tla_export`,
    // whose build errors stop it first.
    match export_output(kind, dialect) {
        Export::Artifact(output, ext) => {
            let content = output
                .gate(&Lifecycle::new(&program, &model, caps))?
                .emit(&model, &View::whole())?;
            let out_path = if out.extension().is_some() {
                out.to_path_buf()
            } else {
                out.with_extension(ext)
            };
            fs::write(&out_path, &content)
                .with_context(|| format!("failed to write {}", out_path.display()))?;
            println!("wrote {}", out_path.display());
            Ok(())
        }
        Export::Tla => {
            let module_name =
                tla_file(out).map_or_else(|| "RdraSpec".to_string(), |(_, stem)| stem);
            let bounds = tla_bounds(instance_count, int_range_max)?;
            let TlaExport {
                whole: bundle,
                state_models,
            } = tla_export(&model, bounds, &module_name)?;
            gate(&Lifecycle::new(&program, &model, caps), Reads::System)?;
            write_tla(out, bundle, state_models)
        }
    }
}

/// `-o X.tla`: the directory the file sits in (`.` for a bare name) and its module name `X`.
pub(crate) fn tla_file(out: &Path) -> Option<(PathBuf, String)> {
    if !out.extension().is_some_and(|e| e == "tla") {
        return None;
    }
    let stem = out
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("RdraSpec")
        .to_string();
    let dir = match out.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    Some((dir, stem))
}

/// Write the bundle where `-o` points: `X.tla` and its `X.cfg`, `X.cfg` and its `X.tla`, or
/// the bundle's own module name under a directory. The state-model modules go beside it.
fn write_tla(out: &Path, bundle: TlaBundle, state_models: Vec<(TlaBundle, String)>) -> Result<()> {
    let ext = out.extension();
    let (tla_path, cfg_path) = if ext.is_some_and(|e| e == "tla") {
        (out.to_path_buf(), out.with_extension("cfg"))
    } else if ext.is_some_and(|e| e == "cfg") {
        (out.with_extension("tla"), out.to_path_buf())
    } else {
        let dir = if out.is_dir() || ext.is_none() {
            out.to_path_buf()
        } else {
            out.parent().unwrap_or(Path::new(".")).to_path_buf()
        };
        if !dir.exists() {
            fs::create_dir_all(&dir)
                .with_context(|| format!("failed to create {}", dir.display()))?;
        }
        (
            dir.join(format!("{}.tla", bundle.module_name)),
            dir.join(format!("{}.cfg", bundle.module_name)),
        )
    };
    let dir = match tla_path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let mut files = vec![(tla_path, bundle.tla), (cfg_path, bundle.cfg)];
    for (sm, witnesses) in state_models {
        files.push((dir.join(format!("{}.tla", sm.module_name)), sm.tla));
        files.push((dir.join(format!("{}.cfg", sm.module_name)), sm.cfg));
        let cfg = sm
            .witness_cfg
            .expect("a state-model module has a witness cfg");
        files.push((dir.join(format!("{witnesses}.cfg")), cfg));
    }
    for (path, text) in &files {
        fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))?;
    }
    for (path, _) in &files {
        println!("wrote {}", path.display());
    }
    Ok(())
}

/// `--instance-count` and `--int-range-max` as the system's bounds.
pub fn tla_bounds(instance_count: u32, int_range_max: u32) -> Result<Bounds> {
    let bounds = checked_bounds(instance_count, int_range_max)?;
    if bounds.instances.get() == 1 {
        eprintln!(
            "warning: --instance-count 1 yields singleton Entity_Ids; multi-entity .along / quantifiers cannot distinguish instances"
        );
    }
    Ok(bounds)
}

/// `--instance-count` and `--int-range-max` as the system's bounds, validated and silent.
pub fn checked_bounds(instance_count: u32, int_range_max: u32) -> Result<Bounds> {
    let Some(instances) = u8::try_from(instance_count).ok().and_then(NonZeroU8::new) else {
        bail!("--instance-count must be between 1 and 255");
    };
    let int_max = match u8::try_from(int_range_max) {
        Ok(0) => bail!(
            "--int-range-max must be >= 1 (IntRange 0..0 is not a useful verification domain)"
        ),
        Ok(n) => n,
        Err(_) => bail!("--int-range-max must be at most 255"),
    };
    Ok(Bounds { instances, int_max })
}

/// What `export --kind tla` writes: the whole system in Check mode, and a module per entity
/// cone with the state-model checks `verify` gets from BFS, each with the stem of its
/// `_witnesses` cfg.
pub struct TlaExport {
    pub whole: TlaBundle,
    pub state_models: Vec<(TlaBundle, String)>,
}

/// The export after the `checked_system` gate. The whole module keeps the name `-o` gives it.
pub fn tla_export(model: &SemanticModel, bounds: Bounds, module_name: &str) -> Result<TlaExport> {
    let built = checked_system(model, bounds, &Scope::ALL)?;
    let whole = tla::print(&built.ts, PrintMode::Check, module_name);
    let mut modules = Modules::default();
    modules.claim(&whole.module_name);
    let state_models = state_model_specs(&built.ts, &mut modules);
    Ok(TlaExport {
        whole,
        state_models,
    })
}

/// One `StateModel_<E>` module per entity cone whose entities have a state model, `E` the
/// first of them: the cone as `verify`'s BFS explores it (`explore_targets_reduced`), so TLC
/// reaches the states the BFS reaches. Entities with one cone share its module, and the
/// reduction keeps the seeds of all of them, as there.
fn state_model_specs(ts: &TransitionSystem, modules: &mut Modules) -> Vec<(TlaBundle, String)> {
    let mut groups: Vec<(Cone, Vec<EntId>)> = Vec::new();
    for e in ts.entity_ids() {
        let cone = ts.target_cone(Target::Entity(e));
        match groups.iter_mut().find(|(c, _)| *c == cone) {
            Some((_, shared)) => shared.push(e),
            None => groups.push((cone, vec![e])),
        }
    }
    let mut out = Vec::new();
    for (cone, shared) in groups {
        let Some(first) = shared
            .iter()
            .find(|e| !ts.state_model_states(**e).is_empty())
        else {
            continue;
        };
        let seeds = shared.iter().map(|e| ts.target_seed(Target::Entity(*e)));
        let sub = ts.reduced_cone(&cone, seeds, Caps::VERIFY);
        let local: Vec<EntId> = shared
            .iter()
            .map(|e| {
                let key = ts.entity(*e).key;
                sub.entity_ids()
                    .find(|x| sub.entity(*x).key == key)
                    .expect("an entity is in its own cone")
            })
            .collect();
        let module = modules.claim(&format!("StateModel_{}", ts.entity(*first).id));
        let witnesses = modules.claim(&format!("{module}_witnesses"));
        out.push((tla::print_state_models(&sub, &local, &module), witnesses));
    }
    out
}

/// The module and cfg names one run writes into its directory. A name an earlier one took
/// gets the first free `_<k>`, k from 2, so no file or its `.states` and `.tmp` dirs
/// overwrite another's; a run whose names collide with nothing keeps every name. Names are
/// kept apart ignoring ASCII case, as a file system that folds case would see them
/// (`Cone_Ready.tla` and `Cone_ready.tla` are one file on macOS and Windows).
#[derive(Default)]
pub(crate) struct Modules(BTreeSet<String>);

impl Modules {
    pub(crate) fn claim(&mut self, want: &str) -> String {
        let base = tla::module_name_of(want);
        let mut name = base.clone();
        let mut k = 2;
        while !self.0.insert(name.to_ascii_lowercase()) {
            name = format!("{base}_{k}");
            k += 1;
        }
        name
    }

    /// The `n`th rerun cfg of `module`: `<module>_rest<n>.cfg`, unless that is another
    /// module's cfg. Rerun names cannot collide with each other: each ends in its own
    /// module's name, `_rest<n>`, and the suffix this adds.
    pub(crate) fn rest_cfg(&self, module: &str, n: usize) -> String {
        let base = format!("{module}_rest{n}");
        let mut name = base.clone();
        let mut k = 2;
        while self.0.contains(&name.to_ascii_lowercase()) {
            name = format!("{base}_{k}");
            k += 1;
        }
        format!("{name}.cfg")
    }
}

/// Builds the transition system of `scope` at `bounds` and reports its diagnostics. Any Error
/// diagnostic stops the command, the gate `states` and `check` apply too; an error the
/// scope defers is in `selection.deferred` and does not.
pub fn checked_system(model: &SemanticModel, bounds: Bounds, scope: &Scope) -> Result<Built> {
    let built = TransitionSystem::build(model, bounds, scope);
    for d in &built.diags {
        if d.diag.severity() == Severity::Info {
            continue;
        }
        let ids: Vec<&str> = d
            .entities
            .iter()
            .map(|ek| model.entities[*ek].id.as_str())
            .collect();
        let message = if ids.is_empty() {
            d.diag.to_string()
        } else {
            format!("{}: {}", ids.join(", "), d.diag)
        };
        eprintln!("{}", Finding::new(&d.diag, &ids, message));
    }
    if has_errors(&built.diags) {
        bail!("the lifecycle model has errors; no TLA+ written");
    }
    Ok(built)
}
