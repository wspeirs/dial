use std::{
    borrow::Cow,
    collections::{BTreeSet, HashMap, HashSet},
};
use crate::{
    grammar::clause::{ClauseIdx, ClauseKind},
    position::Position,
    span::Span,
    table::ParseTable,
    RuleType,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InputLocation {
    Pos(usize),
    Span((usize, usize)),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LineColLocation {
    Pos((usize, usize)),
    Span((usize, usize), (usize, usize)),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ErrorVariant<R> {
    ParsingError {
        positives: Vec<R>,
        negatives: Vec<R>,
    },
    CustomError {
        message: String,
    },
}

impl<R: RuleType> ErrorVariant<R> {
    pub fn message(&self) -> Cow<'_, str> {
        match self {
            ErrorVariant::CustomError { message } => Cow::Borrowed(message),
            ErrorVariant::ParsingError { positives, negatives } => {
                let mut parts = Vec::new();
                if !positives.is_empty() {
                    let formatted: Vec<_> = positives.iter().map(|r| format!("{:?}", r)).collect();
                    parts.push(format!("expected {}", formatted.join(", ")));
                }
                if !negatives.is_empty() {
                    let formatted: Vec<_> = negatives.iter().map(|r| format!("{:?}", r)).collect();
                    parts.push(format!("unexpected {}", formatted.join(", ")));
                }
                if parts.is_empty() {
                    Cow::Borrowed("parsing error")
                } else {
                    Cow::Owned(parts.join("; "))
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Error<R> {
    pub variant: ErrorVariant<R>,
    pub location: InputLocation,
    pub line_col: LineColLocation,
    path: Option<String>,
    line: String,
}

impl<R: RuleType> Error<R> {
    pub fn new_from_pos(variant: ErrorVariant<R>, pos: Position<'_>) -> Self {
        let (line_num, col_num) = pos.line_col();
        let line = pos.line_of().to_string();
        Self {
            variant,
            location: InputLocation::Pos(pos.pos()),
            line_col: LineColLocation::Pos((line_num, col_num)),
            path: None,
            line,
        }
    }

    pub fn new_from_span(variant: ErrorVariant<R>, span: Span<'_>) -> Self {
        let (start_line, start_col) = span.start_pos().line_col();
        let (end_line, end_col) = span.end_pos().line_col();
        let line = span.start_pos().line_of().to_string();
        Self {
            variant,
            location: InputLocation::Span((span.start(), span.end())),
            line_col: LineColLocation::Span((start_line, start_col), (end_line, end_col)),
            path: None,
            line,
        }
    }

    pub fn with_path(mut self, path: &str) -> Self {
        self.path = Some(path.to_string());
        self
    }

    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    pub fn line(&self) -> &str {
        &self.line
    }

    pub fn renamed_rules<F: FnMut(&R) -> String>(self, mut f: F) -> Self {
        let variant = match self.variant {
            ErrorVariant::CustomError { message } => ErrorVariant::CustomError { message },
            ErrorVariant::ParsingError { positives, negatives } => {
                let mut parts = Vec::new();
                if !positives.is_empty() {
                    let formatted: Vec<_> = positives.iter().map(&mut f).collect();
                    parts.push(format!("expected {}", formatted.join(", ")));
                }
                if !negatives.is_empty() {
                    let formatted: Vec<_> = negatives.iter().map(&mut f).collect();
                    parts.push(format!("unexpected {}", formatted.join(", ")));
                }
                let message = if parts.is_empty() {
                    "parsing error".to_string()
                } else {
                    parts.join("; ")
                };
                ErrorVariant::CustomError { message }
            }
        };
        Self { variant, ..self }
    }
}

impl<R: RuleType> std::fmt::Display for Error<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (line_num, col_num) = match self.line_col {
            LineColLocation::Pos((l, c)) => (l, c),
            LineColLocation::Span((l, c), _) => (l, c),
        };
        let line_num_str = line_num.to_string();
        let gutter = line_num_str.len() + 1;
        let spaces = " ".repeat(gutter);
        let pointer_spaces = " ".repeat(col_num.saturating_sub(1));

        let path_prefix = if let Some(p) = &self.path {
            format!("{}:", p)
        } else {
            String::new()
        };

        writeln!(f, " --> {}{}:{}", path_prefix, line_num, col_num)?;
        writeln!(f, "{}|", spaces)?;
        writeln!(f, "{} | {}", line_num_str, self.line)?;
        writeln!(f, "{}| {}{}", spaces, pointer_spaces, "^---")?;
        writeln!(f, "{}|", spaces)?;
        write!(f, "{}= {}", spaces, self.variant.message())
    }
}

impl<R: RuleType> std::error::Error for Error<R> {}

pub(crate) fn diagnose_failure<'g, 'i, R: RuleType>(
    table: &ParseTable<'g, 'i, R>,
) -> Error<R> {
    let start_clause = table.grammar.rule_entry[table.start_rule];
    let mut cache = HashMap::new();
    let mut visiting = HashSet::new();

    let error_pos = progress(
        start_clause,
        0,
        table,
        &mut cache,
        &mut visiting,
    );

    let mut positives_set = BTreeSet::new();
    let mut visited_collect = HashSet::new();

    collect_expected_clauses(
        start_clause,
        0,
        error_pos,
        table,
        &mut cache,
        &mut positives_set,
        &mut visited_collect,
    );

    let positives: Vec<R> = positives_set
        .into_iter()
        .map(|r_idx| (table.to_rule)(r_idx as usize))
        .collect();

    let (line_num, col_num, line_str) =
        crate::meta::error::compute_line_col(table.input, error_pos);

    Error {
        variant: ErrorVariant::ParsingError {
            positives,
            negatives: Vec::new(),
        },
        location: InputLocation::Pos(error_pos),
        line_col: LineColLocation::Pos((line_num, col_num)),
        path: None,
        line: line_str,
    }
}

fn progress<'g, 'i, R: RuleType>(
    clause: ClauseIdx,
    pos: usize,
    table: &ParseTable<'g, 'i, R>,
    cache: &mut HashMap<(u32, u32), usize>,
    visiting: &mut HashSet<(u32, u32)>,
) -> usize {
    let key = (clause, pos as u32);
    if let Some(&res) = cache.get(&key) {
        return res;
    }
    if !visiting.insert(key) {
        return pos;
    }

    let res = if let Some(m) = table.memo.get(pos, clause) {
        pos + table.arena.nodes[m as usize].len as usize
    } else {
        let cl = &table.grammar.clauses[clause as usize];
        match &cl.kind {
            ClauseKind::Seq(subs) => {
                let mut cur = pos;
                let mut reached = pos;
                for &sub in subs.iter() {
                    if let Some(m) = table.memo.get(cur, sub) {
                        cur += table.arena.nodes[m as usize].len as usize;
                        reached = cur;
                    } else {
                        reached = progress(sub, cur, table, cache, visiting);
                        break;
                    }
                }
                reached
            }
            ClauseKind::First(subs) => {
                subs.iter()
                    .map(|&b| progress(b, pos, table, cache, visiting))
                    .max()
                    .unwrap_or(pos)
            }
            ClauseKind::OneOrMore { sub, sep } => {
                let mut cur = pos;
                while let Some(m) = table.memo.get(cur, *sub) {
                    let len = table.arena.nodes[m as usize].len as usize;
                    if len == 0 {
                        break;
                    }
                    cur += len;
                    if let Some(s) = sep {
                        if let Some(ls) = table.memo.get(cur, *s) {
                            cur += table.arena.nodes[ls as usize].len as usize;
                        }
                    }
                }
                progress(*sub, cur, table, cache, visiting)
            }
            ClauseKind::Char(_)
            | ClauseKind::CharRange(_, _)
            | ClauseKind::Str(_)
            | ClauseKind::StrInsens(_)
            | ClauseKind::Any
            | ClauseKind::Soi
            | ClauseKind::Eoi
            | ClauseKind::Nothing
            | ClauseKind::NotFollowedBy(_) => pos,
        }
    };

    visiting.remove(&key);
    cache.insert(key, res);
    res
}

fn collect_expected_clauses<'g, 'i, R: RuleType>(
    clause: ClauseIdx,
    pos: usize,
    error_pos: usize,
    table: &ParseTable<'g, 'i, R>,
    cache: &mut HashMap<(u32, u32), usize>,
    positives: &mut BTreeSet<u32>,
    visited: &mut HashSet<(u32, u32)>,
) {
    let key = (clause, pos as u32);
    if !visited.insert(key) {
        return;
    }

    if pos > error_pos {
        return;
    }

    let cl = &table.grammar.clauses[clause as usize];

    if pos == error_pos {
        if let Some(r) = cl.rule {
            if cl.emit && r != table.start_rule as u32 {
                positives.insert(r);
                return;
            }
        }
        match &cl.kind {
            ClauseKind::Char(_)
            | ClauseKind::CharRange(_, _)
            | ClauseKind::Str(_)
            | ClauseKind::StrInsens(_)
            | ClauseKind::Any
            | ClauseKind::Soi
            | ClauseKind::Eoi => {
                let owner_entry = table.grammar.rule_entry[cl.owner_rule as usize] as usize;
                let is_silent = !table.grammar.clauses[owner_entry].emit;
                if !is_silent && cl.owner_rule != table.start_rule as u32 {
                    positives.insert(cl.owner_rule);
                }
                return;
            }
            _ => {}
        }
    }

    if let Some(m) = table.memo.get(pos, clause) {
        let len = table.arena.nodes[m as usize].len as usize;
        if pos + len < error_pos {
            return;
        }
    }

    match &cl.kind {
        ClauseKind::Seq(subs) => {
            let mut cur = pos;
            for &sub in subs.iter() {
                if let Some(m) = table.memo.get(cur, sub) {
                    let len = table.arena.nodes[m as usize].len as usize;
                    if table.grammar.clauses[sub as usize].can_match_zero_chars && cur == error_pos {
                        let mut dummy = HashSet::new();
                        let prog = progress(sub, cur, table, cache, &mut dummy);
                        if prog >= error_pos {
                            collect_expected_clauses(
                                sub,
                                cur,
                                error_pos,
                                table,
                                cache,
                                positives,
                                visited,
                            );
                        }
                    }
                    cur += len;
                } else {
                    let mut dummy = HashSet::new();
                    let prog = progress(sub, cur, table, cache, &mut dummy);
                    if prog >= error_pos {
                        collect_expected_clauses(
                            sub,
                            cur,
                            error_pos,
                            table,
                            cache,
                            positives,
                            visited,
                        );
                    }
                    if table.grammar.clauses[sub as usize].can_match_zero_chars {
                        continue;
                    }
                    break;
                }
            }
        }
        ClauseKind::First(subs) => {
            for &branch in subs.iter() {
                let mut dummy_visiting = HashSet::new();
                let prog = progress(branch, pos, table, cache, &mut dummy_visiting);
                if prog >= error_pos {
                    collect_expected_clauses(
                        branch,
                        pos,
                        error_pos,
                        table,
                        cache,
                        positives,
                        visited,
                    );
                }
            }
        }
        ClauseKind::OneOrMore { sub, sep } => {
            let mut cur = pos;
            while let Some(m) = table.memo.get(cur, *sub) {
                let len = table.arena.nodes[m as usize].len as usize;
                if len == 0 {
                    break;
                }
                cur += len;
                if let Some(s) = sep {
                    if let Some(ls) = table.memo.get(cur, *s) {
                        cur += table.arena.nodes[ls as usize].len as usize;
                    }
                }
            }
            collect_expected_clauses(*sub, cur, error_pos, table, cache, positives, visited);
        }
        _ => {}
    }
}
