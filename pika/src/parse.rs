use std::{cmp::Reverse, collections::BinaryHeap};
use crate::{
    error::Error,
    grammar::clause::{ClauseIdx, ClauseKind, Grammar},
    iterators::Pairs,
    memo::{Arena, MatchNode, MemoTable, NodeIdx},
    table::ParseTable,
    RuleType,
};

pub fn parse_table<'g, 'i, R: RuleType>(
    grammar: &'g Grammar,
    start_rule: usize,
    input: &'i str,
    to_rule: fn(usize) -> R,
) -> ParseTable<'g, 'i, R> {
    let (memo, arena) = parse_memo(grammar, input);
    ParseTable {
        grammar,
        input,
        memo,
        arena,
        start_rule,
        to_rule,
    }
}

pub fn parse_pairs<'i, R: RuleType>(
    grammar: &Grammar,
    start_rule: usize,
    input: &'i str,
    to_rule: fn(usize) -> R,
) -> Result<Pairs<'i, R>, Error<R>> {
    let table = parse_table(grammar, start_rule, input, to_rule);
    table.pairs()
}

pub(crate) fn table_to_pairs<'g, 'i, R: RuleType>(
    table: &ParseTable<'g, 'i, R>,
) -> Result<Pairs<'i, R>, Error<R>> {
    crate::tree::build_pairs_from_table(table)
}

pub(crate) fn parse_memo<'g, 'i>(grammar: &'g Grammar, input: &'i str) -> (MemoTable, Arena) {
    let mut memo = MemoTable::new(input.len());
    let mut arena = Arena::new();
    let num_clauses = grammar.clauses.len();
    let mut pq = BinaryHeap::new();
    let mut in_queue = vec![false; num_clauses];

    for pos in (0..=input.len()).rev() {
        if !input.is_char_boundary(pos) {
            continue;
        }
        for &t in &grammar.terminals {
            let t_usize = t as usize;
            if !in_queue[t_usize] {
                in_queue[t_usize] = true;
                pq.push(Reverse(t));
            }
        }
        while let Some(Reverse(c)) = pq.pop() {
            in_queue[c as usize] = false;
            let m = match_clause(c, pos, &memo, &mut arena, grammar, input);
            add_match(
                c,
                pos,
                m,
                &mut memo,
                &arena,
                grammar,
                &mut pq,
                &mut in_queue,
            );
        }
    }

    (memo, arena)
}

pub(crate) fn lookup(
    clause: u32,
    pos: usize,
    memo: &MemoTable,
    arena: &mut Arena,
    grammar: &Grammar,
    input: &str,
) -> Option<NodeIdx> {
    if let Some(node) = memo.get(pos, clause) {
        return Some(node);
    }
    if let ClauseKind::NotFollowedBy(_) = &grammar.clauses[clause as usize].kind {
        return match_clause(clause, pos, memo, arena, grammar, input);
    }
    if grammar.clauses[clause as usize].can_match_zero_chars {
        let node = arena.alloc(clause, pos as u32, 0, 0, &[]);
        return Some(node);
    }
    None
}

pub(crate) fn match_clause(
    clause: u32,
    pos: usize,
    memo: &MemoTable,
    arena: &mut Arena,
    grammar: &Grammar,
    input: &str,
) -> Option<NodeIdx> {
    if pos > input.len() {
        return None;
    }
    let cl = &grammar.clauses[clause as usize];
    match &cl.kind {
        ClauseKind::Nothing => Some(arena.alloc(clause, pos as u32, 0, 0, &[])),
        ClauseKind::Char(c) => {
            if input[pos..].starts_with(*c) {
                Some(arena.alloc(clause, pos as u32, c.len_utf8() as u32, 0, &[]))
            } else {
                None
            }
        }
        ClauseKind::CharRange(a, b) => {
            if let Some(ch) = input[pos..].chars().next() {
                if ch >= *a && ch <= *b {
                    Some(arena.alloc(clause, pos as u32, ch.len_utf8() as u32, 0, &[]))
                } else {
                    None
                }
            } else {
                None
            }
        }
        ClauseKind::Str(s) => {
            if input[pos..].starts_with(&**s) {
                Some(arena.alloc(clause, pos as u32, s.len() as u32, 0, &[]))
            } else {
                None
            }
        }
        ClauseKind::StrInsens(s) => {
            if input.len() >= pos + s.len() && input[pos..pos + s.len()].eq_ignore_ascii_case(s) {
                Some(arena.alloc(clause, pos as u32, s.len() as u32, 0, &[]))
            } else {
                None
            }
        }
        ClauseKind::Any => {
            if pos < input.len() {
                let ch = input[pos..].chars().next().unwrap();
                Some(arena.alloc(clause, pos as u32, ch.len_utf8() as u32, 0, &[]))
            } else {
                None
            }
        }
        ClauseKind::Soi => {
            if pos == 0 {
                Some(arena.alloc(clause, 0, 0, 0, &[]))
            } else {
                None
            }
        }
        ClauseKind::Eoi => {
            if pos == input.len() {
                Some(arena.alloc(clause, pos as u32, 0, 0, &[]))
            } else {
                None
            }
        }
        ClauseKind::Seq(subs) => {
            let mut cur = pos;
            let mut sub_matches = Vec::with_capacity(subs.len());
            for &sub in subs.iter() {
                let m = lookup(sub, cur, memo, arena, grammar, input)?;
                let len = arena.nodes[m as usize].len as usize;
                sub_matches.push(m);
                cur += len;
            }
            let total_len = (cur - pos) as u32;
            Some(arena.alloc(clause, pos as u32, total_len, 0, &sub_matches))
        }
        ClauseKind::First(subs) => {
            for (i, &sub) in subs.iter().enumerate() {
                if let Some(m) = lookup(sub, pos, memo, arena, grammar, input) {
                    let len = arena.nodes[m as usize].len;
                    return Some(arena.alloc(clause, pos as u32, len, i as u32, &[m]));
                }
            }
            None
        }
        ClauseKind::OneOrMore { sub, sep } => {
            let m = lookup(*sub, pos, memo, arena, grammar, input)?;
            let m_len = arena.nodes[m as usize].len as usize;
            if m_len == 0 {
                return Some(arena.alloc(clause, pos as u32, 0, 0, &[m]));
            }
            let tail_pos = pos + m_len;
            let mut sub_matches = vec![m];
            if let Some(s) = sep {
                if let Some(ls) = lookup(*s, tail_pos, memo, arena, grammar, input) {
                    let ls_len = arena.nodes[ls as usize].len as usize;
                    let next_pos = tail_pos + ls_len;
                    if let Some(t) = lookup(clause, next_pos, memo, arena, grammar, input) {
                        sub_matches.push(ls);
                        sub_matches.push(t);
                    }
                }
            } else {
                if let Some(t) = lookup(clause, tail_pos, memo, arena, grammar, input) {
                    sub_matches.push(t);
                }
            }
            let last_sub = *sub_matches.last().unwrap();
            let last_node = &arena.nodes[last_sub as usize];
            let total_len = (last_node.start + last_node.len) - pos as u32;
            Some(arena.alloc(clause, pos as u32, total_len, 0, &sub_matches))
        }
        ClauseKind::NotFollowedBy(sub) => {
            if lookup(*sub, pos, memo, arena, grammar, input).is_none() {
                Some(arena.alloc(clause, pos as u32, 0, 0, &[]))
            } else {
                None
            }
        }
    }
}

fn is_better_than(new_node: &MatchNode, old_node: &MatchNode, is_first: bool) -> bool {
    if is_first && new_node.first_sub < old_node.first_sub {
        true
    } else {
        new_node.len > old_node.len
    }
}

fn add_match(
    clause: u32,
    pos: usize,
    m: Option<NodeIdx>,
    memo: &mut MemoTable,
    arena: &Arena,
    grammar: &Grammar,
    pq: &mut BinaryHeap<Reverse<ClauseIdx>>,
    in_queue: &mut [bool],
) {
    let mut match_updated = false;
    if let Some(new_idx) = m {
        let new_node = &arena.nodes[new_idx as usize];
        if let Some(old_idx) = memo.get(pos, clause) {
            let old_node = &arena.nodes[old_idx as usize];
            let is_first = matches!(grammar.clauses[clause as usize].kind, ClauseKind::First(_));
            if is_better_than(new_node, old_node, is_first) {
                memo.put(pos, clause, new_idx);
                match_updated = true;
            }
        } else {
            memo.put(pos, clause, new_idx);
            match_updated = true;
        }
    }

    for &seed_parent in &grammar.clauses[clause as usize].seed_parents {
        if match_updated || grammar.clauses[seed_parent as usize].can_match_zero_chars {
            let p_usize = seed_parent as usize;
            if !in_queue[p_usize] {
                in_queue[p_usize] = true;
                pq.push(Reverse(seed_parent));
            }
        }
    }
}
