//! Source position indexing and spanned syntax errors for tooling and LSP.

use crate::ast::Span;

/// A syntax error that preserves the exact byte span in source code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub message: String,
    pub span: Span,
}

impl SyntaxError {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }
}

impl std::fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (bytes {}..{})", self.message, self.span.start, self.span.end)
    }
}

impl std::error::Error for SyntaxError {}

/// Fast line/column index for byte offsets in source text.
#[derive(Debug, Clone)]
pub struct SourceIndex {
    line_starts: Vec<usize>,
    len: usize,
}

impl SourceIndex {
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self {
            line_starts,
            len: source.len(),
        }
    }

    /// Convert a 0-based byte offset to 0-based `(line, character)` position.
    pub fn offset_to_position(&self, offset: usize) -> (u32, u32) {
        let offset = offset.min(self.len);
        match self.line_starts.binary_search(&offset) {
            Ok(line) => (line as u32, 0),
            Err(next_line) => {
                let line = next_line.saturating_sub(1);
                let col = offset.saturating_sub(self.line_starts[line]);
                (line as u32, col as u32)
            }
        }
    }

    /// Convert a byte `Span` to 0-based start and end `(line, character)` pairs.
    pub fn span_to_range(&self, span: &Span) -> ((u32, u32), (u32, u32)) {
        (
            self.offset_to_position(span.start),
            self.offset_to_position(span.end),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_index_multiline() {
        let source = "let a = 1;\nlet b = 2;\nlet c = 3;";
        let idx = SourceIndex::new(source);

        assert_eq!(idx.offset_to_position(0), (0, 0));
        assert_eq!(idx.offset_to_position(4), (0, 4));
        assert_eq!(idx.offset_to_position(11), (1, 0)); // 'l' in line 2
        assert_eq!(idx.offset_to_position(15), (1, 4)); // 'b' in line 2
        assert_eq!(idx.offset_to_position(22), (2, 0)); // 'l' in line 3
    }
}
