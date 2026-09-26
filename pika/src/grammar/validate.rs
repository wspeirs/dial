use std::collections::{HashMap, HashSet};
use crate::meta::{
    ast::{Expr, Grammar as AstGrammar, Rule as AstRule},
    error::GrammarError,
};

const STACK_OPS: &[&str] = &[
    "PUSH",
    "PUSH_LITERAL",
    "POP",
    "POP_ALL",
    "PEEK",
    "PEEK_ALL",
    "DROP",
];

const SUPPORTED_BUILTINS: &[&str] = &[
    "ANY",
    "SOI",
    "EOI",
    "NEWLINE",
    "ASCII_DIGIT",
    "ASCII_NONZERO_DIGIT",
    "ASCII_BIN_DIGIT",
    "ASCII_OCT_DIGIT",
    "ASCII_HEX_DIGIT",
    "ASCII_ALPHA_LOWER",
    "ASCII_ALPHA_UPPER",
    "ASCII_ALPHA",
    "ASCII_ALPHANUMERIC",
    "ASCII",
];

const UNSUPPORTED_PEST_BUILTINS: &[&str] = &[
    "LETTER",
    "CASED_LETTER",
    "UPPERCASE_LETTER",
    "LOWERCASE_LETTER",
    "TITLECASE_LETTER",
    "MODIFIER_LETTER",
    "OTHER_LETTER",
    "MARK",
    "NONSPACING_MARK",
    "SPACING_MARK",
    "ENCLOSING_MARK",
    "NUMBER",
    "DECIMAL_NUMBER",
    "LETTER_NUMBER",
    "OTHER_NUMBER",
    "PUNCTUATION",
    "CONNECTOR_PUNCTUATION",
    "DASH_PUNCTUATION",
    "OPEN_PUNCTUATION",
    "CLOSE_PUNCTUATION",
    "INITIAL_PUNCTUATION",
    "FINAL_PUNCTUATION",
    "OTHER_PUNCTUATION",
    "SYMBOL",
    "MATH_SYMBOL",
    "CURRENCY_SYMBOL",
    "MODIFIER_SYMBOL",
    "OTHER_SYMBOL",
    "SEPARATOR",
    "SPACE_SEPARATOR",
    "LINE_SEPARATOR",
    "PARAGRAPH_SEPARATOR",
    "OTHER",
    "CONTROL",
    "FORMAT",
    "SURROGATE",
    "PRIVATE_USE",
    "UNASSIGNED",
    "ALPHABETIC",
];

pub fn validate_ast(ast: &AstGrammar, src: &str) -> Result<(), GrammarError> {
    let mut defined_rules = HashSet::new();
    let mut rule_groups: HashMap<&str, Vec<&AstRule>> = HashMap::new();

    for rule in &ast.rules {
        let name = rule.name.as_str();

        if name == "_" || name == "ANY" || name == "SOI" || name == "EOI" || STACK_OPS.contains(&name) {
            return Err(GrammarError::new(
                format!("error: rule name '{}' is reserved", name),
                rule.span.0,
                src,
            ));
        }

        defined_rules.insert(name);
        rule_groups.entry(name).or_default().push(rule);
    }

    for (name, rules) in &rule_groups {
        if rules.len() > 1 {
            let mut levels = HashSet::new();
            for r in rules {
                match &r.prec {
                    Some(prec) => {
                        if !levels.insert(prec.level) {
                            return Err(GrammarError::new(
                                format!(
                                    "error: rule '{}' defined multiple times at precedence level {}",
                                    name, prec.level
                                ),
                                r.span.0,
                                src,
                            ));
                        }
                    }
                    None => {
                        return Err(GrammarError::new(
                            format!("error: rule '{}' is defined multiple times", name),
                            r.span.0,
                            src,
                        ));
                    }
                }
            }
        }
    }

    for rule in &ast.rules {
        if let Some(prec) = &rule.prec {
            if prec.assoc.is_some() && !has_self_ref(&rule.expr, &rule.name) {
                return Err(GrammarError::new(
                    "error: associativity requires a self-reference in the rule body",
                    rule.span.0,
                    src,
                ));
            }
        }

        validate_expr(&rule.expr, &defined_rules, src)?;
    }

    Ok(())
}

fn has_self_ref(expr: &Expr, target: &str) -> bool {
    match expr {
        Expr::Ident(name, _) => name == target,
        Expr::Seq(list) | Expr::Choice(list) => list.iter().any(|e| has_self_ref(e, target)),
        Expr::Opt(e)
        | Expr::Rep(e)
        | Expr::RepOnce(e)
        | Expr::RepExact(e, _)
        | Expr::RepMin(e, _)
        | Expr::RepMax(e, _)
        | Expr::RepMinMax(e, _, _)
        | Expr::PosPred(e)
        | Expr::NegPred(e)
        | Expr::Tag(e, _) => has_self_ref(e, target),
        Expr::Str(_) | Expr::Insens(_) | Expr::Range(_, _) => false,
    }
}

fn validate_expr(
    expr: &Expr,
    defined_rules: &HashSet<&str>,
    src: &str,
) -> Result<(), GrammarError> {
    match expr {
        Expr::Ident(name, span) => {
            if STACK_OPS.contains(&name.as_str()) {
                return Err(GrammarError::new(
                    format!(
                        "error: '{}' is not supported by pika: stack state depends on left context, which is incompatible with position-indexed memoization",
                        name
                    ),
                    span.0,
                    src,
                ));
            }
            if UNSUPPORTED_PEST_BUILTINS.contains(&name.as_str()) {
                return Err(GrammarError::new(
                    format!("error: builtin '{}' is not supported by pika", name),
                    span.0,
                    src,
                ));
            }
            if SUPPORTED_BUILTINS.contains(&name.as_str()) || defined_rules.contains(name.as_str())
            {
                Ok(())
            } else {
                Err(GrammarError::new(
                    format!("undefined rule '{}'", name),
                    span.0,
                    src,
                ))
            }
        }
        Expr::Tag(sub, _) => {
            match sub.as_ref() {
                Expr::Ident(name, _span) if defined_rules.contains(name.as_str()) => {
                    validate_expr(sub, defined_rules, src)
                }
                Expr::Ident(_, span) => Err(GrammarError::new(
                    "error: #tag must be applied to a rule reference",
                    span.0,
                    src,
                )),
                _ => Err(GrammarError::new(
                    "error: #tag must be applied to a rule reference",
                    expr_span(sub),
                    src,
                )),
            }
        }
        Expr::Seq(list) | Expr::Choice(list) => {
            for e in list {
                validate_expr(e, defined_rules, src)?;
            }
            Ok(())
        }
        Expr::Opt(e)
        | Expr::Rep(e)
        | Expr::RepOnce(e)
        | Expr::RepExact(e, _)
        | Expr::RepMin(e, _)
        | Expr::RepMax(e, _)
        | Expr::RepMinMax(e, _, _)
        | Expr::PosPred(e)
        | Expr::NegPred(e) => validate_expr(e, defined_rules, src),
        Expr::Str(_) | Expr::Insens(_) | Expr::Range(_, _) => Ok(()),
    }
}

pub fn expr_span(expr: &Expr) -> usize {
    match expr {
        Expr::Ident(_, span) => span.0,
        Expr::Seq(list) | Expr::Choice(list) => list.first().map(expr_span).unwrap_or(0),
        Expr::Opt(e)
        | Expr::Rep(e)
        | Expr::RepOnce(e)
        | Expr::RepExact(e, _)
        | Expr::RepMin(e, _)
        | Expr::RepMax(e, _)
        | Expr::RepMinMax(e, _, _)
        | Expr::PosPred(e)
        | Expr::NegPred(e)
        | Expr::Tag(e, _) => expr_span(e),
        Expr::Str(_) | Expr::Insens(_) | Expr::Range(_, _) => 0,
    }
}
