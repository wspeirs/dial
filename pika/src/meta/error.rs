#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrammarError {
    pub message: String,
    pub pos: usize,
    pub line_col: (usize, usize),
    pub line: String,
}

impl GrammarError {
    pub fn new(message: impl Into<String>, pos: usize, src: &str) -> Self {
        let (line_num, col_num, line_str) = compute_line_col(src, pos);
        Self {
            message: message.into(),
            pos,
            line_col: (line_num, col_num),
            line: line_str,
        }
    }
}

pub fn compute_line_col(src: &str, pos: usize) -> (usize, usize, String) {
    let pos = pos.min(src.len());
    let mut boundary = pos;
    while !src.is_char_boundary(boundary) && boundary > 0 {
        boundary -= 1;
    }
    let slice = &src[..boundary];
    let mut line_num = 1;
    let mut last_newline_end = 0;
    for (i, b) in slice.bytes().enumerate() {
        if b == b'\n' {
            line_num += 1;
            last_newline_end = i + 1;
        }
    }
    let col = slice[last_newline_end..].chars().count() + 1;
    let line_rest = &src[last_newline_end..];
    let line_end = line_rest
        .find('\n')
        .map(|i| last_newline_end + i)
        .unwrap_or(src.len());
    let mut line_str = &src[last_newline_end..line_end];
    if line_str.ends_with('\r') {
        line_str = &line_str[..line_str.len() - 1];
    }
    (line_num, col, line_str.to_string())
}

impl std::fmt::Display for GrammarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (line, col) = self.line_col;
        let line_num_str = line.to_string();
        let gutter = line_num_str.len() + 1;
        let spaces = " ".repeat(gutter);
        let pointer_spaces = " ".repeat(col.saturating_sub(1));

        writeln!(f, " --> {}:{}", line, col)?;
        writeln!(f, "{}|", spaces)?;
        writeln!(f, "{} | {}", line_num_str, self.line)?;
        writeln!(f, "{}| {}{}", spaces, pointer_spaces, "^---")?;
        writeln!(f, "{}|", spaces)?;
        write!(f, "{}= {}", spaces, self.message)
    }
}

impl std::error::Error for GrammarError {}
