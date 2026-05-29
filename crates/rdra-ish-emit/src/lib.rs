//! rdra-emit: RDRA output emitters (PlantUML, CSV, Mermaid).

pub mod csv;
pub mod mermaid;
pub mod plantuml;
pub mod state_pattern;

use rdra_ish_core::model::SemanticModel;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmitError {
    #[error("CSV write error: {0}")]
    Csv(#[from] ::csv::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// 出力のスコープ
#[derive(Debug, Clone)]
pub enum Scope {
    Whole,
    /// 特定BUC群（buc_id 文字列のリスト、和集合で絞り込む）
    Bucs(Vec<String>),
}

/// 出力フィルタ
#[derive(Debug, Clone)]
pub enum Filter {
    None,
    ActorOnly,
    EntityOnly,
    Er,
}

#[derive(Debug, Clone)]
pub struct View {
    pub scope: Scope,
    pub filter: Filter,
}

impl View {
    pub fn whole() -> Self {
        Self {
            scope: Scope::Whole,
            filter: Filter::None,
        }
    }

    pub fn er() -> Self {
        Self {
            scope: Scope::Whole,
            filter: Filter::Er,
        }
    }

    /// 1つ以上の BUC id を指定して絞り込むビューを作る。
    /// `buc_ids` が空の場合は `Scope::Whole` になる。
    pub fn bucs(buc_ids: Vec<String>) -> Self {
        let scope = if buc_ids.is_empty() {
            Scope::Whole
        } else {
            Scope::Bucs(buc_ids)
        };
        Self {
            scope,
            filter: Filter::None,
        }
    }
}

pub trait Emitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError>;
}
