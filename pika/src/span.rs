use crate::position::Position;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span<'i> {
    pub(crate) input: &'i str,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

impl<'i> Span<'i> {
    pub fn new(input: &'i str, start: usize, end: usize) -> Option<Self> {
        if start <= end && end <= input.len() && input.is_char_boundary(start) && input.is_char_boundary(end) {
            Some(Self { input, start, end })
        } else {
            None
        }
    }

    pub fn get(&self, range: std::ops::Range<usize>) -> Option<Span<'i>> {
        let abs_start = self.start.checked_add(range.start)?;
        let abs_end = self.start.checked_add(range.end)?;
        if abs_end <= self.end {
            Span::new(self.input, abs_start, abs_end)
        } else {
            None
        }
    }

    #[inline]
    pub fn start(&self) -> usize {
        self.start
    }

    #[inline]
    pub fn end(&self) -> usize {
        self.end
    }

    pub fn start_pos(&self) -> Position<'i> {
        Position::new(self.input, self.start).expect("span start must be a valid position")
    }

    pub fn end_pos(&self) -> Position<'i> {
        Position::new(self.input, self.end).expect("span end must be a valid position")
    }

    pub fn split(self) -> (Position<'i>, Position<'i>) {
        (self.start_pos(), self.end_pos())
    }

    #[inline]
    pub fn as_str(&self) -> &'i str {
        &self.input[self.start..self.end]
    }

    #[inline]
    pub fn get_input(&self) -> &'i str {
        self.input
    }

    pub fn lines(&self) -> std::str::Lines<'i> {
        self.as_str().lines()
    }
}

impl<'i> std::fmt::Debug for Span<'i> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Span")
            .field("str", &self.as_str())
            .field("start", &self.start)
            .field("end", &self.end)
            .finish()
    }
}
