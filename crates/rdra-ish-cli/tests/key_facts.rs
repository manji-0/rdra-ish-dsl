//! The phase 5 check: on every `samples.txt` root the key facts cover every key-side
//! relate, every printed foreign key resolves, and each consumer prints exactly the fact set.
//! Agreement is partly tautological while every consumer reads the fact; it is here to catch
//! the next consumer that re-derives keys on its own.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use rdra_ish_core::lifecycle::{Bounds, Scope, TransitionSystem, VarKind};
use rdra_ish_core::model::{NodeRef, RelKind, SemanticModel};
use rdra_ish_core::{build_merged_model, resolve, tx};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn samples() -> Vec<String> {
    std::fs::read_to_string(repo().join("samples.txt"))
        .expect("samples.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn rdra_files(root: &Path, out: &mut Vec<PathBuf>) {
    if root.is_file() {
        out.push(root.to_path_buf());
        return;
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(root)
        .expect("readable sample dir")
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            rdra_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rdra") {
            out.push(path);
        }
    }
}

/// The CLI's include rule: each file's parent and grandparent.
fn load(sample: &str) -> SemanticModel {
    let mut files = Vec::new();
    rdra_files(&repo().join(sample), &mut files);
    let mut includes: Vec<PathBuf> = Vec::new();
    for f in &files {
        let canon = std::fs::canonicalize(f).unwrap_or_else(|_| f.clone());
        if let Some(parent) = canon.parent() {
            for a in [parent, parent.parent().unwrap_or(parent)] {
                if !includes.iter().any(|p| p == a) {
                    includes.push(a.to_path_buf());
                }
            }
        }
    }
    let (program, resolve_diags) = resolve(&files, &includes);
    let (model, diags) = build_merged_model(&program, &includes);
    let errors: Vec<String> = resolve_diags
        .iter()
        .chain(&diags)
        .filter(|d| !d.is_warning)
        .map(|d| d.error.to_string())
        .collect();
    assert!(errors.is_empty(), "{sample}: model errors {errors:?}");
    model
}

/// Runs the CLI with `--out` in a scratch dir and returns what it wrote.
fn export(sample: &str, args: &[&str], file: &str) -> String {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("key_facts")
        .join(sample.replace('/', "__"));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join(file);
    let _ = std::fs::remove_file(&out);
    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .args(args)
        .arg(sample)
        .args(["--out".as_ref(), out.as_os_str()])
        .output()
        .expect("rdra-ish runs");
    assert!(
        output.status.success(),
        "{sample}: `{}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::read_to_string(&out).unwrap_or_else(|e| panic!("{sample}: {}: {e}", out.display()))
}

/// One printed key column: `(child, column, parent, parent column, unique)`.
type Key = (String, String, String, String, bool);

fn fact_keys(model: &SemanticModel) -> Vec<Key> {
    let mut keys: Vec<Key> = model
        .foreign_keys()
        .flat_map(|(_, fk)| {
            let child = &model.entities[fk.child].id;
            let parent = &model.entities[fk.parent].id;
            fk.columns.iter().map(move |(column, parent_column)| {
                (
                    child.clone(),
                    column.clone(),
                    parent.clone(),
                    parent_column.clone(),
                    fk.unique(),
                )
            })
        })
        .collect();
    keys.sort();
    keys
}

fn sorted<T: Ord>(mut items: Vec<T>) -> Vec<T> {
    items.sort();
    items
}

/// Every relate whose child holds the key yields exactly one fact with key columns, the
/// child on the end the cardinality names; no fact comes from anything else.
fn assert_coverage(sample: &str, model: &SemanticModel) {
    let mut by_relation: BTreeMap<usize, usize> = BTreeMap::new();
    for (_, fk) in model.foreign_keys() {
        *by_relation.entry(fk.relation).or_default() += 1;
        assert!(
            !fk.columns.is_empty(),
            "{sample}: relation {} yields a key with no columns",
            fk.relation
        );
    }
    for (index, rel) in model.relations.iter().enumerate() {
        let (NodeRef::Entity(from), NodeRef::Entity(to)) = (&rel.from, &rel.to) else {
            continue;
        };
        let (child, parent) = match rel.kind {
            RelKind::RelateManyToOne | RelKind::RelateOneToOne => (*from, *to),
            RelKind::RelateOneToMany => (*to, *from),
            _ => {
                assert!(
                    !by_relation.contains_key(&index),
                    "{sample}: relation {index} ({:?}) yields a key",
                    rel.kind
                );
                continue;
            }
        };
        let label = format!(
            "{sample}: relate({}, {}, {:?})",
            model.entities[*from].id, model.entities[*to].id, rel.kind
        );
        assert_eq!(by_relation.get(&index), Some(&1), "{label}: facts");
        let (_, fk) = model
            .foreign_keys()
            .find(|(_, fk)| fk.relation == index)
            .unwrap();
        assert_eq!((fk.child, fk.parent), (child, parent), "{label}: key side");
        assert_eq!(
            fk.unique(),
            rel.kind == RelKind::RelateOneToOne,
            "{label}: unique"
        );
    }
}

/// `A.b` or `"A"."b"` at the start of `text`, and the rest after it.
fn dotted(text: &str) -> (String, String, &str) {
    let (table, rest) = ident(text);
    let rest = rest.strip_prefix('.').expect("dotted DBML ref");
    let (column, rest) = ident(rest);
    (table, column, rest)
}

fn ident(text: &str) -> (String, &str) {
    if let Some(quoted) = text.strip_prefix('"') {
        let end = quoted.find('"').expect("closed quote");
        (quoted[..end].to_string(), &quoted[end + 1..])
    } else {
        let end = text
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(text.len());
        (text[..end].to_string(), &text[end..])
    }
}

/// Table name → column names of a DBML document.
fn dbml_tables(dbml: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut tables: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in dbml.lines() {
        if let Some(rest) = line.strip_prefix("Table ") {
            let (name, _) = ident(rest);
            tables.entry(name.clone()).or_default();
            current = Some(name);
        } else if line.starts_with('}') {
            current = None;
        } else if let (Some(table), Some(body)) = (&current, line.strip_prefix("  ")) {
            if body.starts_with(' ') || body.starts_with("indexes") || body.starts_with("Note") {
                continue;
            }
            let (column, _) = ident(body);
            if !column.is_empty() {
                tables.get_mut(table).unwrap().insert(column);
            }
        }
    }
    tables
}

fn assert_dbml(sample: &str, dbml: &str, facts: &[Key]) {
    let tables = dbml_tables(dbml);
    let mut refs: Vec<Key> = Vec::new();
    for line in dbml.lines() {
        let Some(rest) = line.strip_prefix("Ref: ") else {
            continue;
        };
        let (child, column, rest) = dotted(rest);
        let (op, rest) = rest.trim_start().split_at(1);
        let (parent, parent_column, _) = dotted(rest.trim_start());
        for (table, col) in [(&child, &column), (&parent, &parent_column)] {
            assert!(
                tables.get(table).is_some_and(|cols| cols.contains(col)),
                "{sample}: unresolved DBML ref `{line}`: no {table}.{col}"
            );
        }
        refs.push((child, column, parent, parent_column, op == "-"));
    }
    assert_eq!(sorted(refs), facts, "{sample}: DBML refs");
}

fn assert_json_schema(sample: &str, json: &str, facts: &[Key]) {
    let doc: serde_json::Value = serde_json::from_str(json).expect("JSON Schema parses");
    let mut keys = Vec::new();
    for def in doc["$defs"].as_object().expect("$defs").values() {
        if def["x-rdra-ish-kind"] != "entity" {
            continue;
        }
        let entity = def["x-rdra-ish-id"].as_str().unwrap();
        for (column, prop) in def["properties"].as_object().unwrap() {
            if prop["x-rdra-ish-foreign-key"] != true {
                continue;
            }
            keys.push((
                entity.to_string(),
                column.clone(),
                prop["x-rdra-ish-fk-target"].as_str().unwrap().to_string(),
                prop["x-rdra-ish-unique"] == true,
            ));
        }
    }
    let want: Vec<_> = facts
        .iter()
        .map(|(c, col, p, _, u)| (c.clone(), col.clone(), p.clone(), *u))
        .collect();
    assert_eq!(sorted(keys), sorted(want), "{sample}: JSON Schema keys");
}

/// One CSV record, honouring double-quoted fields.
fn csv_fields(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                fields.last_mut().unwrap().push('"');
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(String::new()),
            _ => fields.last_mut().unwrap().push(c),
        }
    }
    fields
}

fn assert_entity_csv(sample: &str, csv: &str, facts: &[Key]) {
    let mut lines = csv.lines();
    let header = csv_fields(lines.next().expect("CSV header"));
    let at = |name: &str| header.iter().position(|h| h == name).expect(name);
    let (entity, column, is_fk, target, unique) = (
        at("entity_id"),
        at("column_name"),
        at("is_fk"),
        at("fk_target"),
        at("is_unique"),
    );
    let rows: Vec<_> = lines
        .map(csv_fields)
        .filter(|row| row[is_fk] == "true")
        .map(|row| {
            (
                row[entity].clone(),
                row[column].clone(),
                row[target].clone(),
                row[unique] == "true",
            )
        })
        .collect();
    let want: Vec<_> = facts
        .iter()
        .map(|(c, col, p, _, u)| (c.clone(), col.clone(), p.clone(), *u))
        .collect();
    assert_eq!(sorted(rows), sorted(want), "{sample}: entity CSV FK rows");
}

fn assert_er(sample: &str, mermaid: &str, facts: &[Key]) {
    let mut markers = Vec::new();
    let mut current: Option<&str> = None;
    for line in mermaid.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_suffix(" {") {
            current = Some(name);
        } else if trimmed == "}" {
            current = None;
        } else if let (Some(entity), Some(rest)) = (current, trimmed.strip_suffix(" FK")) {
            let column = rest.rsplit(' ').next().unwrap();
            markers.push((entity.to_string(), column.to_string()));
        }
    }
    let want: Vec<_> = facts
        .iter()
        .map(|(c, col, ..)| (c.clone(), col.clone()))
        .collect();
    assert_eq!(sorted(markers), sorted(want), "{sample}: ER FK markers");
}

fn assert_tx_edges(sample: &str, model: &SemanticModel) {
    let edges: Vec<_> = tx::fk_edges(model).collect();
    let want: Vec<_> = model
        .foreign_keys()
        .map(|(_, fk)| (fk.parent, fk.child))
        .collect();
    assert_eq!(sorted(edges), sorted(want), "{sample}: tx FK edges");
}

/// One owner var per fact whose child and parent are both lifecycle entities.
fn assert_owner_vars(sample: &str, model: &SemanticModel) {
    let ts = TransitionSystem::build(model, Bounds::STATES, &Scope::All).ts;
    let entities = ts.entities();
    let owners: Vec<_> = ts
        .vars()
        .iter()
        .filter_map(|var| match var.kind {
            VarKind::Owner {
                child, parent, fk, ..
            } => Some((
                fk,
                entities[child.index()].key,
                entities[parent.index()].key,
            )),
            _ => None,
        })
        .collect();
    let in_scope: BTreeSet<_> = entities.iter().map(|e| e.key).collect();
    let want: Vec<_> = model
        .foreign_keys()
        .filter(|(_, fk)| in_scope.contains(&fk.child) && in_scope.contains(&fk.parent))
        .map(|(id, fk)| (id, fk.child, fk.parent))
        .collect();
    assert_eq!(
        sorted(owners),
        sorted(want),
        "{sample}: lifecycle owner vars"
    );
}

#[test]
fn key_facts_hold_on_every_sample() {
    for sample in samples() {
        let model = load(&sample);
        assert_coverage(&sample, &model);
        let facts = fact_keys(&model);

        let dbml = export(&sample, &["export", "--kind", "dbml"], "schema.dbml");
        assert_dbml(&sample, &dbml, &facts);
        let json = export(&sample, &["export", "--kind", "json-schema"], "schema.json");
        assert_json_schema(&sample, &json, &facts);
        let csv = export(&sample, &["csv", "--kind", "entity"], "entity.csv");
        assert_entity_csv(&sample, &csv, &facts);
        let er = export(&sample, &["export", "--kind", "mermaid-er"], "er.mmd");
        assert_er(&sample, &er, &facts);

        assert_tx_edges(&sample, &model);
        assert_owner_vars(&sample, &model);
    }
}
