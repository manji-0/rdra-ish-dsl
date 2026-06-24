use chumsky::error::Simple;

use crate::token::Token;

/// Human-readable message for a chumsky parse error.
pub fn format_parse_error(err: &Simple<Token>) -> String {
    format!("{err:?}")
}
