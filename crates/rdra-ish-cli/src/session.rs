//! A model loaded once, with its lifecycle explored at most once and only for an output
//! that reads it (the state diagram), for every `diagram` and `list` output drawn from it.
//! Each command loads one. The diagram contract tests (`tests/diagram_contract.rs`) keep one
//! per sample and draw every scope and kind from it: a `diagram` process parses the model
//! again for each, and explores the lifecycle again for each state diagram.

use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::Parser;
use rdra_ish_core::lifecycle::Caps;
use rdra_ish_core::{ResolvedProgram, SemanticModel};

use crate::cli::{Cli, Commands, ListFormat, ListKind};
use crate::lifecycle_gate::{gate, Exploration, Lifecycle};
use crate::list_output::{format_rules, format_transitions, list_elements};
use crate::load::load_clean;
use crate::memory::generator_caps;

/// A loaded model, refused when it has errors as every generator refuses it.
#[doc(hidden)]
pub struct Session {
    pub(crate) program: ResolvedProgram,
    pub(crate) model: SemanticModel,
    /// `memory::generator_caps`: what the lifecycle is explored at.
    caps: Caps,
    /// The inputs as a diagram header names them, `model=<path>[,<path>...]`.
    pub(crate) model_path: String,
    explored: Exploration,
}

impl Session {
    pub fn load(inputs: &[PathBuf]) -> Result<Self> {
        let shown = inputs
            .iter()
            .map(|input| input.display().to_string())
            .collect::<Vec<_>>()
            .join(",");
        Self::load_as(inputs, shown)
    }

    /// Loads `inputs`, which a diagram header names `model_path`: the path a caller in
    /// another directory would pass.
    pub fn load_as(inputs: &[PathBuf], model_path: String) -> Result<Self> {
        let (program, model, manifest) = load_clean(inputs)?;
        Ok(Session {
            program,
            model,
            caps: generator_caps(manifest.as_ref()),
            model_path,
            explored: Exploration::new(),
        })
    }

    pub(crate) fn lifecycle(&self) -> Lifecycle<'_> {
        Lifecycle::sharing(&self.program, &self.model, self.caps, &self.explored)
    }

    /// What `rdra-ish diagram <model> <args>` prints. `--out` and `--all` write files,
    /// and are refused here.
    pub fn diagram(&self, args: &[&str]) -> Result<String> {
        let Commands::Diagram(args) = self.parse("diagram", args)? else {
            unreachable!("parsed as `diagram`")
        };
        if args.all || args.out.is_some() {
            bail!("--out and --all write files; Session::diagram returns one diagram's text");
        }
        Ok(crate::diagram::draw(self, args)?.expect("no --out, so the text is returned"))
    }

    /// What `rdra-ish list <model> <args>` prints.
    pub fn list(&self, args: &[&str]) -> Result<String> {
        let Commands::List { kind, format, .. } = self.parse("list", args)? else {
            unreachable!("parsed as `list`")
        };
        if matches!(kind, ListKind::Surface) && !matches!(format, ListFormat::Table) {
            bail!("`list --kind surface` prints canonical text only; drop `--format`");
        }
        self.list_text(&kind, &format)
    }

    pub(crate) fn list_text(&self, kind: &ListKind, format: &ListFormat) -> Result<String> {
        gate(&self.lifecycle(), kind.reads())?;
        Ok(match kind {
            ListKind::Rule => format_rules(&self.model, Some(&self.program), format)?,
            ListKind::Transition => format_transitions(&self.model, Some(&self.program), format)?,
            _ => list_elements(&self.model, kind, format)?,
        })
    }

    /// `rdra-ish <command> <model> <args>`, parsed as the CLI parses it.
    fn parse(&self, command: &str, args: &[&str]) -> Result<Commands> {
        let argv = ["rdra-ish", command, &self.model_path]
            .into_iter()
            .chain(args.iter().copied());
        Ok(Cli::try_parse_from(argv)?.command)
    }
}
