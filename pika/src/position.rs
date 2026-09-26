use crate::span::Span;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position<'i> {
    pub(crate) input: &'i str,
    pub(crate) pos: usize,
}

impl<'i> Position<'i> {
    pub fn new(input: &'i str, pos: usize) -> Option<Self> {
        if pos <= input.len() && input.is_char_boundary(pos) {
            Some(Self { input, pos })
        } else {
            None
        }
    }

    pub fn from_start(input: &'i str) -> Self {
        Self { input, pos: 0 }
    }

    #[inline]
    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn span(&self, other: &Position<'i>) -> Span<'i> {
        let (start, end) = if self.pos <= other.pos {
            (self.pos, other.pos)
        } else {
            (other.pos, self.pos)
        };
        Span {
            input: self.input,
            start,
            end,
        }
    }

    pub fn line_col(&self) -> (usize, usize) {
        let (line, col, _) = crate::meta::error::compute_line_col(self.input, self.pos);
        (line, col)
    }

    pub fn line_of(&self) -> &'i str {
        let line_start = self.input[..self.pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let line_end = self.input[self.pos..].find('\n').map(|i| self.pos + i).unwrap_or(self.input.len());
        let mut line = &self.input[line_start..line_end];
        if line.ends_with('\r') {
            line = &line[..line.len() - 1];
        }
        line
    }
}

impl<'i> std::fmt::Debug for Position<'i> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Position")
            .field("pos", &self.pos)
            .finish()
    }
}
