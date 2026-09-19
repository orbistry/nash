use nash_region::{Located, Position};
use nash_source::Expr;

use crate::Parser;
use crate::error;

impl<'a> Parser<'a> {
    pub(crate) fn bytes(
        &mut self,
        start: Position,
    ) -> Result<&'a Located<Expr<'a>>, error::Expr<'a>> {
        let bytes = self.bytes_literal(error::Expr::Start, error::Expr::Bytes)?;
        Ok(self.add_end(start, Expr::Bytes(bytes)))
    }
}

#[cfg(test)]
mod tests {
    use crate::expression::{assert_expr_error_snapshot, assert_expr_snapshot};

    #[test]
    fn empty() {
        assert_expr_snapshot!("#\"\"");
    }

    #[test]
    fn lower_hex() {
        assert_expr_snapshot!("#\"ff00\"");
    }

    #[test]
    fn mixed_hex() {
        assert_expr_snapshot!("#\"DEADbeef\"");
    }

    #[test]
    fn error_odd_length() {
        assert_expr_error_snapshot!("#\"f\"");
    }

    #[test]
    fn error_bad_digit() {
        assert_expr_error_snapshot!("#\"zz\"");
    }

    #[test]
    fn error_endless() {
        assert_expr_error_snapshot!("#\"ab");
    }
}
