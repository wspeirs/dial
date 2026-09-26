use crate::{
    error::Error,
    grammar::clause::Grammar,
    iterators::{Pair, Pairs},
    memo::{Arena, MemoTable},
    span::Span,
    RuleType,
};

pub struct ParseTable<'g, 'i, R: RuleType> {
    pub(crate) grammar: &'g Grammar,
    pub(crate) input: &'i str,
    pub(crate) memo: MemoTable,
    pub(crate) arena: Arena,
    pub(crate) start_rule: usize,
    pub(crate) to_rule: fn(usize) -> R,
}

impl<'g, 'i, R: RuleType> ParseTable<'g, 'i, R> {
    pub fn input(&self) -> &'i str {
        self.input
    }

    pub fn pairs(&self) -> Result<Pairs<'i, R>, Error<R>> {
        crate::parse::table_to_pairs(self)
    }

    fn rule_id(&self, rule: R) -> Option<usize> {
        (0..self.grammar.rule_names.len()).find(|&i| (self.to_rule)(i) == rule)
    }

    fn clauses_for_rule(&self, rule: R) -> Vec<u32> {
        let rule_id = match self.rule_id(rule) {
            Some(id) => id,
            None => return Vec::new(),
        };
        let entry_clause = self.grammar.rule_entry[rule_id];
        let prec_group = self.grammar.clauses[entry_clause as usize].prec_group;

        let mut res = Vec::new();
        for (idx, c) in self.grammar.clauses.iter().enumerate() {
            let matches_rule = c.rule == Some(rule_id as u32);
            let matches_prec = prec_group.is_some() && c.prec_group == prec_group;
            if matches_rule || matches_prec || idx == entry_clause as usize {
                res.push(idx as u32);
            }
        }
        res
    }

    fn longest_match_at(&self, pos: usize, clauses: &[u32]) -> Option<(u32, usize)> {
        let mut best: Option<(u32, usize)> = None;
        for &c in clauses {
            if let Some(node_idx) = self.memo.get(pos, c) {
                let len = self.arena.nodes[node_idx as usize].len as usize;
                if len > 0 {
                    match best {
                        Some((_, best_len)) if len <= best_len => {}
                        _ => best = Some((node_idx, len)),
                    }
                }
            }
        }
        best
    }

    pub fn matches_of(&self, rule: R) -> Vec<Pair<'i, R>> {
        let clauses = self.clauses_for_rule(rule);
        let mut res = Vec::new();
        for pos in 0..=self.input.len() {
            if !self.input.is_char_boundary(pos) {
                continue;
            }
            if let Some((node_idx, _)) = self.longest_match_at(pos, &clauses) {
                res.push(crate::tree::build_pair_for_node(self, node_idx, rule));
            }
        }
        res
    }

    pub fn next_match_after(&self, rule: R, pos: usize) -> Option<Pair<'i, R>> {
        let clauses = self.clauses_for_rule(rule);
        for cur in pos..=self.input.len() {
            if !self.input.is_char_boundary(cur) {
                continue;
            }
            if let Some((node_idx, _)) = self.longest_match_at(cur, &clauses) {
                return Some(crate::tree::build_pair_for_node(self, node_idx, rule));
            }
        }
        None
    }

    pub fn unmatched_regions(&self, rules: &[R]) -> Vec<Span<'i>> {
        let mut all_clauses = Vec::new();
        for &r in rules {
            all_clauses.extend(self.clauses_for_rule(r));
        }
        all_clauses.sort_unstable();
        all_clauses.dedup();

        let mut spans = Vec::new();
        let mut pos = 0;

        while pos < self.input.len() {
            if let Some((_, len)) = self.longest_match_at(pos, &all_clauses) {
                if len > 0 {
                    pos += len;
                    continue;
                }
            }

            let gap_start = pos;
            pos += 1;
            while pos < self.input.len() {
                if self.input.is_char_boundary(pos) {
                    if let Some((_, len)) = self.longest_match_at(pos, &all_clauses) {
                        if len > 0 {
                            break;
                        }
                    }
                }
                pos += 1;
            }

            if gap_start < pos {
                spans.push(Span {
                    input: self.input,
                    start: gap_start,
                    end: pos,
                });
            }
        }

        spans
    }
}
