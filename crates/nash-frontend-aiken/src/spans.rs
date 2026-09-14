use aiken_lang::ast::Span;
use nash_region::{Position, Region};

pub(crate) struct Spans {
    starts: Vec<usize>,
    len: usize,
}

impl Spans {
    pub(crate) fn new(source: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            source
                .bytes()
                .enumerate()
                .filter_map(|(i, byte)| (byte == b'\n').then_some(i + 1)),
        );
        Self {
            starts,
            len: source.len(),
        }
    }

    fn position(&self, offset: usize) -> Position {
        let offset = offset.min(self.len);
        let line = self.starts.partition_point(|start| *start <= offset) - 1;
        Position::new(line + 1, offset - self.starts[line] + 1)
    }

    pub(crate) fn region(&self, span: Span) -> Region {
        Region::new(
            self.position(span.start),
            self.position(span.end.max(span.start)),
        )
    }
}
