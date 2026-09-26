use std::collections::{HashMap, HashSet};
use crate::{
    grammar::{
        clause::{Clause, ClauseIdx, ClauseKind, Grammar},
        validate::{expr_span, validate_ast},
    },
    meta::{
        ast::{Assoc, Expr, Grammar as AstGrammar, Rule as AstRule, RuleTy},
        error::GrammarError,
        parse_grammar,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Ctx {
    Normal,
    Atomic,
    Compound,
}

#[derive(Clone, Debug)]
struct InternalRule {
    name: String,
    owner_rule_id: u32,
    top_rule_id: Option<u32>,
    prec_group: Option<u32>,
    ty: RuleTy,
    expr: Expr,
}

pub fn compile_grammar(src: &str) -> Result<Grammar, GrammarError> {
    let ast = parse_grammar(src)?;
    validate_ast(&ast, src)?;

    // 1. Collect distinct rule names in declaration order, starting with EOI at index 0.
    let mut rule_names = vec!["EOI".to_string()];
    for rule in &ast.rules {
        if !rule_names.contains(&rule.name) {
            rule_names.push(rule.name.clone());
        }
    }

    // 2. Precedence rewriting
    let (internal_rules, prec_entries) = rewrite_precedence(&ast, &rule_names);

    // 3. Check nullability of repetitions and WHITESPACE/COMMENT on AST
    validate_nullability(&ast, &internal_rules, src)?;

    // 4. Construct clause graph with atomicity instantiation
    let mut builder = GrammarBuilder::new(rule_names, internal_rules, prec_entries);
    builder.build()?;

    Ok(builder.finish())
}

fn rewrite_precedence(
    ast: &AstGrammar,
    rule_names: &[String],
) -> (Vec<InternalRule>, HashMap<String, String>) {
    let mut rules_by_name: HashMap<&str, Vec<&AstRule>> = HashMap::new();
    for rule in &ast.rules {
        rules_by_name.entry(&rule.name).or_default().push(rule);
    }

    let mut internal_rules = Vec::new();
    let mut prec_entries = HashMap::new();
    let mut next_prec_group = 0u32;

    for (rule_idx, rule_name) in rule_names.iter().enumerate() {
        if rule_name == "EOI" {
            continue;
        }
        let defs = match rules_by_name.get(rule_name.as_str()) {
            Some(d) => d,
            None => continue,
        };

        if defs.len() == 1 && defs[0].prec.is_none() {
            let r = defs[0];
            internal_rules.push(InternalRule {
                name: r.name.clone(),
                owner_rule_id: rule_idx as u32,
                top_rule_id: Some(rule_idx as u32),
                prec_group: None,
                ty: r.ty,
                expr: r.expr.clone(),
            });
            prec_entries.insert(r.name.clone(), r.name.clone());
        } else {
            let prec_group = next_prec_group;
            next_prec_group += 1;

            let mut sorted_defs: Vec<&AstRule> = defs.clone();
            sorted_defs.sort_by_key(|r| r.prec.as_ref().map(|p| p.level).unwrap_or(0));

            let k = sorted_defs.len() - 1;
            let level_names: Vec<String> = sorted_defs
                .iter()
                .enumerate()
                .map(|(i, _)| format!("{}@p{}", rule_name, i))
                .collect();

            prec_entries.insert(rule_name.clone(), level_names[0].clone());

            for (i, def) in sorted_defs.iter().enumerate() {
                let current_name = &level_names[i];
                let is_highest = i == k;
                let next_name = if !is_highest {
                    Some(&level_names[i + 1])
                } else {
                    None
                };
                let lowest_name = &level_names[0];

                let assoc = def.prec.as_ref().and_then(|p| p.assoc);
                let rewritten_body = rewrite_self_refs(
                    &def.expr,
                    rule_name,
                    current_name,
                    next_name,
                    lowest_name,
                    is_highest,
                    assoc,
                );

                let final_body = if let Some(nxt) = next_name {
                    Expr::Choice(vec![rewritten_body, Expr::Ident(nxt.clone(), def.span)])
                } else {
                    rewritten_body
                };

                internal_rules.push(InternalRule {
                    name: current_name.clone(),
                    owner_rule_id: rule_idx as u32,
                    top_rule_id: Some(rule_idx as u32),
                    prec_group: Some(prec_group),
                    ty: def.ty,
                    expr: final_body,
                });
            }
        }
    }

    (internal_rules, prec_entries)
}

fn rewrite_self_refs(
    expr: &Expr,
    rule_name: &str,
    current_name: &str,
    next_name: Option<&String>,
    lowest_name: &str,
    is_highest: bool,
    assoc: Option<Assoc>,
) -> Expr {
    if is_highest {
        return map_self_refs(expr, rule_name, |_| {
            Expr::Ident(lowest_name.to_string(), (0, 0))
        });
    }

    match assoc {
        Some(Assoc::Left) => {
            let mut first_seen = false;
            map_self_refs(expr, rule_name, |_| {
                if !first_seen {
                    first_seen = true;
                    Expr::Ident(current_name.to_string(), (0, 0))
                } else {
                    Expr::Ident(next_name.unwrap().clone(), (0, 0))
                }
            })
        }
        Some(Assoc::Right) => {
            let total = count_self_refs(expr, rule_name);
            let mut count = 0;
            map_self_refs(expr, rule_name, |_| {
                count += 1;
                if count == total {
                    Expr::Ident(current_name.to_string(), (0, 0))
                } else {
                    Expr::Ident(next_name.unwrap().clone(), (0, 0))
                }
            })
        }
        None => map_self_refs(expr, rule_name, |span| {
            let cur = Expr::Ident(current_name.to_string(), span);
            let nxt = Expr::Ident(next_name.unwrap().clone(), span);
            Expr::Choice(vec![cur, nxt])
        }),
    }
}

fn count_self_refs(expr: &Expr, target: &str) -> usize {
    match expr {
        Expr::Ident(name, _) => {
            if name == target {
                1
            } else {
                0
            }
        }
        Expr::Seq(list) | Expr::Choice(list) => {
            list.iter().map(|e| count_self_refs(e, target)).sum()
        }
        Expr::Opt(e)
        | Expr::Rep(e)
        | Expr::RepOnce(e)
        | Expr::RepExact(e, _)
        | Expr::RepMin(e, _)
        | Expr::RepMax(e, _)
        | Expr::RepMinMax(e, _, _)
        | Expr::PosPred(e)
        | Expr::NegPred(e)
        | Expr::Tag(e, _) => count_self_refs(e, target),
        Expr::Str(_) | Expr::Insens(_) | Expr::Range(_, _) => 0,
    }
}

fn map_self_refs<F: FnMut((usize, usize)) -> Expr>(
    expr: &Expr,
    target: &str,
    mut f: F,
) -> Expr {
    fn helper<F: FnMut((usize, usize)) -> Expr>(expr: &Expr, target: &str, f: &mut F) -> Expr {
        match expr {
            Expr::Ident(name, span) => {
                if name == target {
                    f(*span)
                } else {
                    expr.clone()
                }
            }
            Expr::Seq(list) => {
                Expr::Seq(list.iter().map(|e| helper(e, target, f)).collect())
            }
            Expr::Choice(list) => {
                Expr::Choice(list.iter().map(|e| helper(e, target, f)).collect())
            }
            Expr::Opt(e) => Expr::Opt(Box::new(helper(e, target, f))),
            Expr::Rep(e) => Expr::Rep(Box::new(helper(e, target, f))),
            Expr::RepOnce(e) => Expr::RepOnce(Box::new(helper(e, target, f))),
            Expr::RepExact(e, n) => Expr::RepExact(Box::new(helper(e, target, f)), *n),
            Expr::RepMin(e, n) => Expr::RepMin(Box::new(helper(e, target, f)), *n),
            Expr::RepMax(e, n) => Expr::RepMax(Box::new(helper(e, target, f)), *n),
            Expr::RepMinMax(e, m, n) => {
                Expr::RepMinMax(Box::new(helper(e, target, f)), *m, *n)
            }
            Expr::PosPred(e) => Expr::PosPred(Box::new(helper(e, target, f))),
            Expr::NegPred(e) => Expr::NegPred(Box::new(helper(e, target, f))),
            Expr::Tag(e, t) => Expr::Tag(Box::new(helper(e, target, f)), t.clone()),
            Expr::Str(_) | Expr::Insens(_) | Expr::Range(_, _) => expr.clone(),
        }
    }
    helper(expr, target, &mut f)
}

fn validate_nullability(
    ast: &AstGrammar,
    internal_rules: &[InternalRule],
    src: &str,
) -> Result<(), GrammarError> {
    // Fixed point nullability calculation
    let mut nullability: HashMap<String, bool> = HashMap::new();
    for r in internal_rules {
        nullability.insert(r.name.clone(), false);
    }

    loop {
        let mut changed = false;
        for r in internal_rules {
            let cur = *nullability.get(&r.name).unwrap_or(&false);
            let next = eval_nullable(&r.expr, &nullability);
            if next && !cur {
                nullability.insert(r.name.clone(), true);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // Check WHITESPACE and COMMENT
    for rule in &ast.rules {
        if rule.name == "WHITESPACE" || rule.name == "COMMENT" {
            let is_null = eval_nullable(&rule.expr, &nullability);
            if is_null {
                return Err(GrammarError::new(
                    format!("error: rule '{}' can match the empty string", rule.name),
                    rule.span.0,
                    src,
                ));
            }
        }
    }

    // Check repetitions inside all AST rules
    for rule in &ast.rules {
        check_repetition_nullability(&rule.expr, &nullability, src)?;
    }

    Ok(())
}

fn eval_nullable(expr: &Expr, nullability: &HashMap<String, bool>) -> bool {
    match expr {
        Expr::Str(s) | Expr::Insens(s) => s.is_empty(),
        Expr::Range(_, _) => false,
        Expr::Ident(name, _) => {
            if name == "SOI" || name == "EOI" || name == "ANY" {
                false
            } else {
                *nullability.get(name).unwrap_or(&false)
            }
        }
        Expr::Seq(list) => list.iter().all(|e| eval_nullable(e, nullability)),
        Expr::Choice(list) => list.iter().any(|e| eval_nullable(e, nullability)),
        Expr::Opt(_) => true,
        Expr::Rep(_) => true,
        Expr::RepOnce(e) => eval_nullable(e, nullability),
        Expr::RepExact(e, n) => *n == 0 || eval_nullable(e, nullability),
        Expr::RepMin(e, n) => *n == 0 || eval_nullable(e, nullability),
        Expr::RepMax(_, _) => true,
        Expr::RepMinMax(e, m, _) => *m == 0 || eval_nullable(e, nullability),
        Expr::PosPred(_) | Expr::NegPred(_) => true,
        Expr::Tag(e, _) => eval_nullable(e, nullability),
    }
}

fn check_repetition_nullability(
    expr: &Expr,
    nullability: &HashMap<String, bool>,
    src: &str,
) -> Result<(), GrammarError> {
    match expr {
        Expr::Rep(sub) | Expr::RepOnce(sub) | Expr::RepMin(sub, _) => {
            if eval_nullable(sub, nullability) {
                return Err(GrammarError::new(
                    "error: expression inside repetition can match the empty string",
                    expr_span(sub),
                    src,
                ));
            }
            check_repetition_nullability(sub, nullability, src)
        }
        Expr::RepMinMax(sub, m, n) => {
            if *n > *m && eval_nullable(sub, nullability) {
                return Err(GrammarError::new(
                    "error: expression inside repetition can match the empty string",
                    expr_span(sub),
                    src,
                ));
            }
            check_repetition_nullability(sub, nullability, src)
        }
        Expr::Seq(list) | Expr::Choice(list) => {
            for e in list {
                check_repetition_nullability(e, nullability, src)?;
            }
            Ok(())
        }
        Expr::Opt(e)
        | Expr::RepExact(e, _)
        | Expr::RepMax(e, _)
        | Expr::PosPred(e)
        | Expr::NegPred(e)
        | Expr::Tag(e, _) => check_repetition_nullability(e, nullability, src),
        Expr::Str(_) | Expr::Insens(_) | Expr::Range(_, _) | Expr::Ident(_, _) => Ok(()),
    }
}

struct GrammarBuilder {
    rule_names: Vec<String>,
    internal_rules: HashMap<String, InternalRule>,
    prec_entries: HashMap<String, String>,
    tags: Vec<String>,
    raw_clauses: Vec<Clause>,
    interned: HashMap<(ClauseKind, Option<u32>, bool, Option<u32>, Option<u32>, u32), ClauseIdx>,
    rule_slots: HashMap<(String, Ctx), ClauseIdx>,
    skip_clause: Option<ClauseIdx>,
    lowest_prec_clauses: Vec<ClauseIdx>,
    // (reserved wrapper slot, target rule-reference slot, tag id, owner rule) —
    // resolved after all rule slots are populated, since the target may not yet
    // be backpatched when the tag expression is compiled (HashSet iteration order
    // over `reachable` is unspecified, and cyclic/recursive references make a
    // dependency-ordered compile pass impossible in general).
    pending_tags: Vec<(ClauseIdx, ClauseIdx, u32, u32)>,
}

impl GrammarBuilder {
    fn new(
        rule_names: Vec<String>,
        internal_rules: Vec<InternalRule>,
        prec_entries: HashMap<String, String>,
    ) -> Self {
        let mut rules_map = HashMap::new();
        for r in internal_rules {
            rules_map.insert(r.name.clone(), r);
        }
        Self {
            rule_names,
            internal_rules: rules_map,
            prec_entries,
            tags: Vec::new(),
            raw_clauses: Vec::new(),
            interned: HashMap::new(),
            rule_slots: HashMap::new(),
            skip_clause: None,
            lowest_prec_clauses: Vec::new(),
            pending_tags: Vec::new(),
        }
    }

    fn tag_idx(&mut self, name: &str) -> u32 {
        if let Some(pos) = self.tags.iter().position(|t| t == name) {
            pos as u32
        } else {
            let pos = self.tags.len() as u32;
            self.tags.push(name.to_string());
            pos
        }
    }

    fn intern_or_create(
        &mut self,
        kind: ClauseKind,
        rule: Option<u32>,
        emit: bool,
        prec_group: Option<u32>,
        tag: Option<u32>,
        owner_rule: u32,
    ) -> ClauseIdx {
        let key = (kind.clone(), rule, emit, prec_group, tag, owner_rule);
        if let Some(&idx) = self.interned.get(&key) {
            return idx;
        }

        let idx = self.raw_clauses.len() as ClauseIdx;
        self.raw_clauses.push(Clause {
            kind: kind.clone(),
            rule,
            emit,
            prec_group,
            tag,
            owner_rule,
            can_match_zero_chars: false,
            seed_parents: Box::new([]),
            idx,
        });
        self.interned.insert(key, idx);
        idx
    }

    fn reserve_clause(&mut self) -> ClauseIdx {
        let idx = self.raw_clauses.len() as ClauseIdx;
        self.raw_clauses.push(Clause {
            kind: ClauseKind::Nothing,
            rule: None,
            emit: false,
            prec_group: None,
            tag: None,
            owner_rule: 0,
            can_match_zero_chars: false,
            seed_parents: Box::new([]),
            idx,
        });
        idx
    }

    fn build(&mut self) -> Result<(), GrammarError> {
        // 2. Discover all reachable (rule_name, ctx) pairs
        let mut reachable = HashSet::new();
        let mut worklist = Vec::new();

        for name in self.internal_rules.keys() {
            let key = (name.clone(), Ctx::Normal);
            if reachable.insert(key.clone()) {
                worklist.push(key);
            }
        }

        while let Some((name, ctx)) = worklist.pop() {
            let rule = match self.internal_rules.get(&name) {
                Some(r) => r,
                None => continue,
            };
            let child_ctx = match rule.ty {
                RuleTy::NonAtomic => Ctx::Normal,
                RuleTy::Atomic => Ctx::Atomic,
                RuleTy::CompoundAtomic => Ctx::Compound,
                _ => ctx,
            };
            let mut refs = Vec::new();
            collect_rule_refs(&rule.expr, &mut refs);
            for r_name in refs {
                let resolved = self.prec_entries.get(&r_name).cloned().unwrap_or(r_name);
                let next_key = (resolved, child_ctx);
                if reachable.insert(next_key.clone()) {
                    worklist.push(next_key);
                }
            }
        }

        // 3. Pre-allocate clause slots for each reachable (rule_name, ctx)
        for key in &reachable {
            let slot = self.reserve_clause();
            self.rule_slots.insert(key.clone(), slot);
        }

        // 4. Build SKIP clause if WHITESPACE or COMMENT is defined
        let has_ws = self.internal_rules.contains_key("WHITESPACE");
        let has_comment = self.internal_rules.contains_key("COMMENT");
        if has_ws || has_comment {
            let mut parts = Vec::new();
            if has_ws {
                let ws_slot = self.rule_slots[&("WHITESPACE".to_string(), Ctx::Normal)];
                // Inside SKIP, WHITESPACE should not emit pairs
                parts.push(ws_slot);
            }
            if has_comment {
                let comment_slot = self.rule_slots[&("COMMENT".to_string(), Ctx::Normal)];
                parts.push(comment_slot);
            }
            let sub = if parts.len() == 1 {
                parts[0]
            } else {
                self.intern_or_create(
                    ClauseKind::First(parts.into_boxed_slice()),
                    None,
                    false,
                    None,
                    None,
                    0,
                )
            };
            let one_or_more = self.intern_or_create(
                ClauseKind::OneOrMore { sub, sep: None },
                None,
                false,
                None,
                None,
                0,
            );
            let nothing =
                self.intern_or_create(ClauseKind::Nothing, None, false, None, None, 0);
            self.skip_clause = Some(self.intern_or_create(
                ClauseKind::First(vec![one_or_more, nothing].into_boxed_slice()),
                None,
                false,
                None,
                None,
                0,
            ));
        }

        // 5. Compile each reachable (rule_name, ctx) into its reserved slot
        for (name, ctx) in &reachable {
            let slot = self.rule_slots[&(name.clone(), *ctx)];
            let rule = self.internal_rules[name].clone();

            let emit = (rule.ty != RuleTy::Silent) && (*ctx != Ctx::Atomic);
            let child_ctx = match rule.ty {
                RuleTy::NonAtomic => Ctx::Normal,
                RuleTy::Atomic => Ctx::Atomic,
                RuleTy::CompoundAtomic => Ctx::Compound,
                _ => *ctx,
            };

            // Compile body. `body_clause` may itself be a bare reference to another
            // rule's slot (e.g. `X = { Y }` compiles straight through to Y's slot,
            // or `#tag = Y` reserves a not-yet-backpatched wrapper) that is not
            // guaranteed to be populated yet: `reachable` is a HashSet, so this loop
            // visits rules in unspecified (non-deterministic across runs) order, and
            // recursive/cyclic references make a dependency-ordered pass impossible
            // in general. Wrap by INDEX (never by copying `.kind`/`.rule`/`.emit`
            // out of `body_clause` right now) so this slot stays correct regardless
            // of when `body_clause`'s own target is finalized — matching semantics
            // are unaffected since Seq of one sub has the same span/length as sub.
            let is_skip_rule = name == "WHITESPACE" || name == "COMMENT";
            let body_clause = self.compile_expr(&rule.expr, child_ctx, rule.owner_rule_id, is_skip_rule);
            self.raw_clauses[slot as usize] = Clause {
                kind: ClauseKind::Seq(Box::new([body_clause])),
                rule: rule.top_rule_id,
                emit,
                prec_group: rule.prec_group,
                tag: None,
                owner_rule: rule.owner_rule_id,
                can_match_zero_chars: false,
                seed_parents: Box::new([]),
                idx: slot,
            };

            if rule.name.ends_with("@p0") && *ctx == Ctx::Normal {
                self.lowest_prec_clauses.push(slot);
            }
        }

        // Resolve deferred `#tag = rule_ref` wrappers now that every rule slot in
        // `reachable` has been populated above (see the comment on `pending_tags`).
        let pending_tags = std::mem::take(&mut self.pending_tags);
        for (wrapper, target, tag_id, owner_rule) in pending_tags {
            let target_clause = self.raw_clauses[target as usize].clone();
            self.raw_clauses[wrapper as usize] = Clause {
                kind: target_clause.kind,
                rule: target_clause.rule,
                emit: target_clause.emit,
                prec_group: target_clause.prec_group,
                tag: Some(tag_id),
                owner_rule,
                can_match_zero_chars: false,
                seed_parents: Box::new([]),
                idx: wrapper,
            };
        }

        Ok(())
    }

    fn compile_expr(
        &mut self,
        expr: &Expr,
        ctx: Ctx,
        owner_rule: u32,
        is_skip_internal: bool,
    ) -> ClauseIdx {
        match expr {
            Expr::Str(s) => {
                if s.is_empty() {
                    self.intern_or_create(ClauseKind::Nothing, None, false, None, None, owner_rule)
                } else if s.chars().count() == 1 {
                    self.intern_or_create(
                        ClauseKind::Char(s.chars().next().unwrap()),
                        None,
                        false,
                        None,
                        None,
                        owner_rule,
                    )
                } else {
                    self.intern_or_create(
                        ClauseKind::Str(s.clone().into_boxed_str()),
                        None,
                        false,
                        None,
                        None,
                        owner_rule,
                    )
                }
            }
            Expr::Insens(s) => {
                if s.is_empty() {
                    self.intern_or_create(ClauseKind::Nothing, None, false, None, None, owner_rule)
                } else {
                    self.intern_or_create(
                        ClauseKind::StrInsens(s.clone().into_boxed_str()),
                        None,
                        false,
                        None,
                        None,
                        owner_rule,
                    )
                }
            }
            Expr::Range(a, b) => self.intern_or_create(
                ClauseKind::CharRange(*a, *b),
                None,
                false,
                None,
                None,
                owner_rule,
            ),
            Expr::Ident(name, _) => self.compile_ident(name, ctx, owner_rule),
            Expr::Choice(list) => {
                let subs: Vec<ClauseIdx> = list
                    .iter()
                    .map(|e| self.compile_expr(e, ctx, owner_rule, is_skip_internal))
                    .collect();
                self.intern_or_create(
                    ClauseKind::First(subs.into_boxed_slice()),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                )
            }
            Expr::Seq(list) => {
                let mut subs = Vec::new();
                for (i, e) in list.iter().enumerate() {
                    if i > 0 && ctx == Ctx::Normal && !is_skip_internal && self.skip_clause.is_some() {
                        subs.push(self.skip_clause.unwrap());
                    }
                    subs.push(self.compile_expr(e, ctx, owner_rule, is_skip_internal));
                }
                if subs.len() == 1 {
                    subs[0]
                } else {
                    self.intern_or_create(
                        ClauseKind::Seq(subs.into_boxed_slice()),
                        None,
                        false,
                        None,
                        None,
                        owner_rule,
                    )
                }
            }
            Expr::Opt(sub) => {
                let s = self.compile_expr(sub, ctx, owner_rule, is_skip_internal);
                let nothing =
                    self.intern_or_create(ClauseKind::Nothing, None, false, None, None, owner_rule);
                self.intern_or_create(
                    ClauseKind::First(vec![s, nothing].into_boxed_slice()),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                )
            }
            Expr::Rep(sub) => {
                let s = self.compile_expr(sub, ctx, owner_rule, is_skip_internal);
                let sep = if ctx == Ctx::Normal && !is_skip_internal {
                    self.skip_clause
                } else {
                    None
                };
                let one_or_more = self.intern_or_create(
                    ClauseKind::OneOrMore { sub: s, sep },
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                );
                let nothing =
                    self.intern_or_create(ClauseKind::Nothing, None, false, None, None, owner_rule);
                self.intern_or_create(
                    ClauseKind::First(vec![one_or_more, nothing].into_boxed_slice()),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                )
            }
            Expr::RepOnce(sub) => {
                let s = self.compile_expr(sub, ctx, owner_rule, is_skip_internal);
                let sep = if ctx == Ctx::Normal && !is_skip_internal {
                    self.skip_clause
                } else {
                    None
                };
                self.intern_or_create(
                    ClauseKind::OneOrMore { sub: s, sep },
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                )
            }
            Expr::RepExact(sub, n) => {
                if *n == 0 {
                    self.intern_or_create(ClauseKind::Nothing, None, false, None, None, owner_rule)
                } else if *n == 1 {
                    self.compile_expr(sub, ctx, owner_rule, is_skip_internal)
                } else {
                    let mut subs = Vec::new();
                    for i in 0..*n {
                        if i > 0 && ctx == Ctx::Normal && !is_skip_internal && self.skip_clause.is_some() {
                            subs.push(self.skip_clause.unwrap());
                        }
                        subs.push(self.compile_expr(sub, ctx, owner_rule, is_skip_internal));
                    }
                    self.intern_or_create(
                        ClauseKind::Seq(subs.into_boxed_slice()),
                        None,
                        false,
                        None,
                        None,
                        owner_rule,
                    )
                }
            }
            Expr::RepMin(sub, n) => {
                let rep = Expr::Rep(sub.clone());
                if *n == 0 {
                    self.compile_expr(&rep, ctx, owner_rule, is_skip_internal)
                } else {
                    let mut list = Vec::new();
                    for _ in 0..*n {
                        list.push((**sub).clone());
                    }
                    list.push(rep);
                    self.compile_expr(&Expr::Seq(list), ctx, owner_rule, is_skip_internal)
                }
            }
            Expr::RepMax(sub, n) => {
                self.compile_rep_min_max(sub, 0, *n, ctx, owner_rule, is_skip_internal)
            }
            Expr::RepMinMax(sub, m, n) => {
                self.compile_rep_min_max(sub, *m, *n, ctx, owner_rule, is_skip_internal)
            }
            Expr::PosPred(sub) => {
                let s = self.compile_expr(sub, ctx, owner_rule, is_skip_internal);
                let neg = self.intern_or_create(
                    ClauseKind::NotFollowedBy(s),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                );
                self.intern_or_create(
                    ClauseKind::NotFollowedBy(neg),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                )
            }
            Expr::NegPred(sub) => {
                let s = self.compile_expr(sub, ctx, owner_rule, is_skip_internal);
                self.intern_or_create(
                    ClauseKind::NotFollowedBy(s),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                )
            }
            Expr::Tag(sub, tag_name) => {
                let tag_id = self.tag_idx(tag_name);
                let inner = self.compile_expr(sub, ctx, owner_rule, is_skip_internal);
                // `inner` is a rule slot (validated: #tag only applies to rule
                // references) that may not be backpatched with its real body yet,
                // since `reachable` is compiled in unspecified HashSet order and
                // recursive references make a dependency-ordered pass impossible.
                // Reserve a placeholder and resolve it once every rule slot in
                // this grammar has been populated (see `build`).
                let wrapper = self.reserve_clause();
                self.pending_tags.push((wrapper, inner, tag_id, owner_rule));
                wrapper
            }
        }
    }

    fn compile_rep_min_max(
        &mut self,
        sub: &Expr,
        m: u32,
        n: u32,
        ctx: Ctx,
        owner_rule: u32,
        is_skip_internal: bool,
    ) -> ClauseIdx {
        if n == 0 {
            return self.intern_or_create(ClauseKind::Nothing, None, false, None, None, owner_rule);
        }
        if m == 0 && n == 1 {
            let opt = Expr::Opt(Box::new(sub.clone()));
            return self.compile_expr(&opt, ctx, owner_rule, is_skip_internal);
        }
        // Desugar as: sub x m followed by (n-m) nested optionals
        let mut tail = self.intern_or_create(ClauseKind::Nothing, None, false, None, None, owner_rule);
        for _ in 0..(n - m) {
            let s = self.compile_expr(sub, ctx, owner_rule, is_skip_internal);
            let seq = if tail == self.intern_or_create(ClauseKind::Nothing, None, false, None, None, owner_rule) {
                s
            } else {
                let mut parts = vec![s];
                if ctx == Ctx::Normal && !is_skip_internal && self.skip_clause.is_some() {
                    parts.push(self.skip_clause.unwrap());
                }
                parts.push(tail);
                self.intern_or_create(
                    ClauseKind::Seq(parts.into_boxed_slice()),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                )
            };
            let nothing =
                self.intern_or_create(ClauseKind::Nothing, None, false, None, None, owner_rule);
            tail = self.intern_or_create(
                ClauseKind::First(vec![seq, nothing].into_boxed_slice()),
                None,
                false,
                None,
                None,
                owner_rule,
            );
        }

        if m == 0 {
            tail
        } else {
            let mut list = Vec::new();
            for _ in 0..m {
                list.push(sub.clone());
            }
            let exact = Expr::Seq(list);
            let head = self.compile_expr(&exact, ctx, owner_rule, is_skip_internal);
            if tail == self.intern_or_create(ClauseKind::Nothing, None, false, None, None, owner_rule) {
                head
            } else {
                let mut parts = vec![head];
                if ctx == Ctx::Normal && !is_skip_internal && self.skip_clause.is_some() {
                    parts.push(self.skip_clause.unwrap());
                }
                parts.push(tail);
                self.intern_or_create(
                    ClauseKind::Seq(parts.into_boxed_slice()),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                )
            }
        }
    }

    fn compile_ident(&mut self, name: &str, ctx: Ctx, owner_rule: u32) -> ClauseIdx {
        match name {
            "ANY" => {
                self.intern_or_create(ClauseKind::Any, None, false, None, None, owner_rule)
            }
            "SOI" => {
                self.intern_or_create(ClauseKind::Soi, None, false, None, None, owner_rule)
            }
            "EOI" => {
                self.intern_or_create(ClauseKind::Eoi, Some(0), true, None, None, owner_rule)
            }
            "NEWLINE" => {
                let s1 = self.intern_or_create(
                    ClauseKind::Str("\r\n".into()),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                );
                let s2 = self.intern_or_create(
                    ClauseKind::Char('\n'),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                );
                let s3 = self.intern_or_create(
                    ClauseKind::Char('\r'),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                );
                self.intern_or_create(
                    ClauseKind::First(vec![s1, s2, s3].into_boxed_slice()),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                )
            }
            "ASCII_DIGIT" => self.intern_or_create(
                ClauseKind::CharRange('0', '9'),
                None,
                false,
                None,
                None,
                owner_rule,
            ),
            "ASCII_NONZERO_DIGIT" => self.intern_or_create(
                ClauseKind::CharRange('1', '9'),
                None,
                false,
                None,
                None,
                owner_rule,
            ),
            "ASCII_BIN_DIGIT" => self.intern_or_create(
                ClauseKind::CharRange('0', '1'),
                None,
                false,
                None,
                None,
                owner_rule,
            ),
            "ASCII_OCT_DIGIT" => self.intern_or_create(
                ClauseKind::CharRange('0', '7'),
                None,
                false,
                None,
                None,
                owner_rule,
            ),
            "ASCII_HEX_DIGIT" => {
                let c1 = self.intern_or_create(
                    ClauseKind::CharRange('0', '9'),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                );
                let c2 = self.intern_or_create(
                    ClauseKind::CharRange('a', 'f'),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                );
                let c3 = self.intern_or_create(
                    ClauseKind::CharRange('A', 'F'),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                );
                self.intern_or_create(
                    ClauseKind::First(vec![c1, c2, c3].into_boxed_slice()),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                )
            }
            "ASCII_ALPHA_LOWER" => self.intern_or_create(
                ClauseKind::CharRange('a', 'z'),
                None,
                false,
                None,
                None,
                owner_rule,
            ),
            "ASCII_ALPHA_UPPER" => self.intern_or_create(
                ClauseKind::CharRange('A', 'Z'),
                None,
                false,
                None,
                None,
                owner_rule,
            ),
            "ASCII_ALPHA" => {
                let c1 = self.intern_or_create(
                    ClauseKind::CharRange('a', 'z'),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                );
                let c2 = self.intern_or_create(
                    ClauseKind::CharRange('A', 'Z'),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                );
                self.intern_or_create(
                    ClauseKind::First(vec![c1, c2].into_boxed_slice()),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                )
            }
            "ASCII_ALPHANUMERIC" => {
                let c1 = self.intern_or_create(
                    ClauseKind::CharRange('a', 'z'),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                );
                let c2 = self.intern_or_create(
                    ClauseKind::CharRange('A', 'Z'),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                );
                let c3 = self.intern_or_create(
                    ClauseKind::CharRange('0', '9'),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                );
                self.intern_or_create(
                    ClauseKind::First(vec![c1, c2, c3].into_boxed_slice()),
                    None,
                    false,
                    None,
                    None,
                    owner_rule,
                )
            }
            "ASCII" => self.intern_or_create(
                ClauseKind::CharRange('\0', '\x7f'),
                None,
                false,
                None,
                None,
                owner_rule,
            ),
            _ => {
                // Rule reference: resolve target rule entered from current context `ctx`
                let target_name = self.prec_entries.get(name).cloned().unwrap_or_else(|| name.to_string());
                self.rule_slots[&(target_name, ctx)]
            }
        }
    }

    fn finish(mut self) -> Grammar {
        // Collect entry clauses for all public rules (rule_names)
        let mut rule_entry = Vec::new();
        // Index 0 is EOI
        let eoi_slot = self.intern_or_create(ClauseKind::Eoi, Some(0), true, None, None, 0);
        rule_entry.push(eoi_slot);

        for name in &self.rule_names[1..] {
            let entry_name = self.prec_entries.get(name).cloned().unwrap_or_else(|| name.clone());
            let slot = self.rule_slots[&(entry_name, Ctx::Normal)];
            rule_entry.push(slot);
        }

        // Topological sort (paper Listing 1)
        let sorted_order = self.topo_sort(&rule_entry);

        // Remap clause indices
        let mut old_to_new = vec![0u32; self.raw_clauses.len()];
        for (new_idx, &old_idx) in sorted_order.iter().enumerate() {
            old_to_new[old_idx as usize] = new_idx as u32;
        }

        let mut final_clauses = Vec::with_capacity(sorted_order.len());
        for (new_idx, &old_idx) in sorted_order.iter().enumerate() {
            let mut c = self.raw_clauses[old_idx as usize].clone();
            c.idx = new_idx as u32;
            c.kind = remap_kind(&c.kind, &old_to_new);
            final_clauses.push(c);
        }

        let final_rule_entry: Vec<ClauseIdx> = rule_entry
            .iter()
            .map(|&idx| old_to_new[idx as usize])
            .collect();

        // Compute can_match_zero_chars in bottom-up topological order
        for i in 0..final_clauses.len() {
            final_clauses[i].can_match_zero_chars = match &final_clauses[i].kind {
                ClauseKind::Nothing | ClauseKind::NotFollowedBy(_) => true,
                ClauseKind::Soi
                | ClauseKind::Eoi
                | ClauseKind::Char(_)
                | ClauseKind::CharRange(_, _)
                | ClauseKind::Str(_)
                | ClauseKind::StrInsens(_)
                | ClauseKind::Any => false,
                ClauseKind::Seq(subs) => {
                    subs.iter().all(|&s| final_clauses[s as usize].can_match_zero_chars)
                }
                ClauseKind::First(subs) => {
                    subs.iter().any(|&s| final_clauses[s as usize].can_match_zero_chars)
                }
                ClauseKind::OneOrMore { sub, .. } => {
                    final_clauses[*sub as usize].can_match_zero_chars
                }
            };
        }

        // Compute seed parents (Listing 9 & 11)
        let mut seed_parents: Vec<Vec<ClauseIdx>> = vec![Vec::new(); final_clauses.len()];
        for parent_idx in 0..final_clauses.len() {
            let p_u32 = parent_idx as u32;
            match &final_clauses[parent_idx].kind {
                ClauseKind::Seq(subs) => {
                    let mut added = HashSet::new();
                    for &sub in subs.iter() {
                        if added.insert(sub) && !seed_parents[sub as usize].contains(&p_u32) {
                            seed_parents[sub as usize].push(p_u32);
                        }
                        if !final_clauses[sub as usize].can_match_zero_chars {
                            break;
                        }
                    }
                }
                ClauseKind::First(subs) => {
                    let mut added = HashSet::new();
                    for &sub in subs.iter() {
                        if added.insert(sub) && !seed_parents[sub as usize].contains(&p_u32) {
                            seed_parents[sub as usize].push(p_u32);
                        }
                    }
                }
                ClauseKind::OneOrMore { sub, .. } => {
                    if !seed_parents[*sub as usize].contains(&p_u32) {
                        seed_parents[*sub as usize].push(p_u32);
                    }
                }
                ClauseKind::NotFollowedBy(sub) => {
                    if !seed_parents[*sub as usize].contains(&p_u32) {
                        seed_parents[*sub as usize].push(p_u32);
                    }
                }
                ClauseKind::Nothing
                | ClauseKind::Char(_)
                | ClauseKind::CharRange(_, _)
                | ClauseKind::Str(_)
                | ClauseKind::StrInsens(_)
                | ClauseKind::Any
                | ClauseKind::Soi
                | ClauseKind::Eoi => {}
            }
        }

        for (i, parents) in seed_parents.into_iter().enumerate() {
            final_clauses[i].seed_parents = parents.into_boxed_slice();
        }

        // Terminals: all terminal clauses except Nothing
        let mut terminals = Vec::new();
        for (i, c) in final_clauses.iter().enumerate() {
            match c.kind {
                ClauseKind::Char(_)
                | ClauseKind::CharRange(_, _)
                | ClauseKind::Str(_)
                | ClauseKind::StrInsens(_)
                | ClauseKind::Any
                | ClauseKind::Soi
                | ClauseKind::Eoi => {
                    terminals.push(i as u32);
                }
                _ => {}
            }
        }

        Grammar {
            clauses: final_clauses,
            terminals,
            rule_names: self.rule_names,
            rule_entry: final_rule_entry,
            tags: self.tags,
        }
    }

    fn topo_sort(&self, rule_entries: &[ClauseIdx]) -> Vec<ClauseIdx> {
        let mut all_clauses_unordered = Vec::new();
        let mut top_level_visited = HashSet::new();

        for &entry in rule_entries {
            self.find_reachable(entry, &mut top_level_visited, &mut all_clauses_unordered);
        }

        let mut top_level_clauses: HashSet<ClauseIdx> =
            all_clauses_unordered.iter().copied().collect();
        for &clause in &all_clauses_unordered {
            for sub in self.clause_subs(clause) {
                top_level_clauses.remove(&sub);
            }
        }

        let mut dfs_roots: Vec<ClauseIdx> = top_level_clauses.iter().copied().collect();
        dfs_roots.extend(self.lowest_prec_clauses.iter().copied());

        let mut cycle_discovered = HashSet::new();
        let mut cycle_finished = HashSet::new();
        let mut cycle_head_clauses = HashSet::new();

        for &clause in &top_level_clauses {
            self.find_cycle_heads(
                clause,
                &mut cycle_discovered,
                &mut cycle_finished,
                &mut cycle_head_clauses,
            );
        }
        for &clause in rule_entries {
            self.find_cycle_heads(
                clause,
                &mut cycle_discovered,
                &mut cycle_finished,
                &mut cycle_head_clauses,
            );
        }

        dfs_roots.extend(cycle_head_clauses.iter().copied());

        let mut all_clauses = Vec::new();
        let mut reachable_visited = HashSet::new();
        for root in dfs_roots {
            self.find_reachable(root, &mut reachable_visited, &mut all_clauses);
        }

        all_clauses
    }

    fn clause_subs(&self, idx: ClauseIdx) -> Vec<ClauseIdx> {
        match &self.raw_clauses[idx as usize].kind {
            ClauseKind::Nothing
            | ClauseKind::Char(_)
            | ClauseKind::CharRange(_, _)
            | ClauseKind::Str(_)
            | ClauseKind::StrInsens(_)
            | ClauseKind::Any
            | ClauseKind::Soi
            | ClauseKind::Eoi => Vec::new(),
            ClauseKind::Seq(subs) | ClauseKind::First(subs) => subs.to_vec(),
            ClauseKind::OneOrMore { sub, sep } => {
                let mut v = vec![*sub];
                if let Some(s) = sep {
                    v.push(*s);
                }
                v
            }
            ClauseKind::NotFollowedBy(sub) => vec![*sub],
        }
    }

    fn find_reachable(
        &self,
        clause: ClauseIdx,
        visited: &mut HashSet<ClauseIdx>,
        order_out: &mut Vec<ClauseIdx>,
    ) {
        if visited.insert(clause) {
            for sub in self.clause_subs(clause) {
                self.find_reachable(sub, visited, order_out);
            }
            order_out.push(clause);
        }
    }

    fn find_cycle_heads(
        &self,
        clause: ClauseIdx,
        discovered: &mut HashSet<ClauseIdx>,
        finished: &mut HashSet<ClauseIdx>,
        cycle_heads: &mut HashSet<ClauseIdx>,
    ) {
        discovered.insert(clause);
        for sub in self.clause_subs(clause) {
            if discovered.contains(&sub) {
                cycle_heads.insert(sub);
            } else if !finished.contains(&sub) {
                self.find_cycle_heads(sub, discovered, finished, cycle_heads);
            }
        }
        discovered.remove(&clause);
        finished.insert(clause);
    }
}

fn collect_rule_refs(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::Ident(name, _) => {
            if !is_builtin(name) {
                out.push(name.clone());
            }
        }
        Expr::Seq(list) | Expr::Choice(list) => {
            for e in list {
                collect_rule_refs(e, out);
            }
        }
        Expr::Opt(e)
        | Expr::Rep(e)
        | Expr::RepOnce(e)
        | Expr::RepExact(e, _)
        | Expr::RepMin(e, _)
        | Expr::RepMax(e, _)
        | Expr::RepMinMax(e, _, _)
        | Expr::PosPred(e)
        | Expr::NegPred(e)
        | Expr::Tag(e, _) => collect_rule_refs(e, out),
        Expr::Str(_) | Expr::Insens(_) | Expr::Range(_, _) => {}
    }
}

fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "ANY"
            | "SOI"
            | "EOI"
            | "NEWLINE"
            | "ASCII_DIGIT"
            | "ASCII_NONZERO_DIGIT"
            | "ASCII_BIN_DIGIT"
            | "ASCII_OCT_DIGIT"
            | "ASCII_HEX_DIGIT"
            | "ASCII_ALPHA_LOWER"
            | "ASCII_ALPHA_UPPER"
            | "ASCII_ALPHA"
            | "ASCII_ALPHANUMERIC"
            | "ASCII"
    )
}

fn remap_kind(kind: &ClauseKind, old_to_new: &[ClauseIdx]) -> ClauseKind {
    match kind {
        ClauseKind::Nothing
        | ClauseKind::Char(_)
        | ClauseKind::CharRange(_, _)
        | ClauseKind::Str(_)
        | ClauseKind::StrInsens(_)
        | ClauseKind::Any
        | ClauseKind::Soi
        | ClauseKind::Eoi => kind.clone(),
        ClauseKind::Seq(subs) => {
            let remapped: Vec<ClauseIdx> = subs.iter().map(|&s| old_to_new[s as usize]).collect();
            ClauseKind::Seq(remapped.into_boxed_slice())
        }
        ClauseKind::First(subs) => {
            let remapped: Vec<ClauseIdx> = subs.iter().map(|&s| old_to_new[s as usize]).collect();
            ClauseKind::First(remapped.into_boxed_slice())
        }
        ClauseKind::OneOrMore { sub, sep } => ClauseKind::OneOrMore {
            sub: old_to_new[*sub as usize],
            sep: sep.map(|s| old_to_new[s as usize]),
        },
        ClauseKind::NotFollowedBy(sub) => ClauseKind::NotFollowedBy(old_to_new[*sub as usize]),
    }
}
