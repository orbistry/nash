use crate::error;
use crate::number::hex_value;
use crate::{Col, Parser, Row};

impl<'a> Parser<'a> {
    /// Parse `#"..."` with an even number of hex digits into decoded bytes.
    pub fn bytes_literal<E>(
        &mut self,
        to_expectation: impl FnOnce(Row, Col) -> E,
        to_error: impl FnOnce(error::Bytes, Row, Col) -> E,
    ) -> Result<&'a [u8], E> {
        let (row, col) = self.position();
        if self.peek() != Some(b'#') || self.peek_at(1) != Some(b'"') {
            return Err(to_expectation(row, col));
        }

        self.advance_by(2);
        let start_pos = self.pos;
        loop {
            match self.peek() {
                None | Some(b'\n') => {
                    return Err(to_error(error::Bytes::Endless, self.row(), self.col()));
                }
                Some(b'"') => break,
                Some(b) if b.is_ascii_hexdigit() => self.advance(),
                Some(_) => {
                    return Err(to_error(
                        error::Bytes::BadHexDigit(self.col()),
                        self.row(),
                        self.col(),
                    ));
                }
            }
        }

        let hex = &self.src[start_pos..self.pos];
        self.advance();
        if !hex.len().is_multiple_of(2) {
            return Err(to_error(error::Bytes::OddLength, row, col));
        }

        Ok(self.bump.alloc_slice_fill_iter(
            hex.chunks(2)
                .map(|pair| (hex_value(pair[0]) << 4) | hex_value(pair[1])),
        ))
    }
}
