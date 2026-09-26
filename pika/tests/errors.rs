use pika::{error::InputLocation, parse_pairs, Grammar};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum RuleF {
    EOI = 0,
    F = 1,
}

fn to_rule_f(idx: usize) -> RuleF {
    match idx {
        0 => RuleF::EOI,
        1 => RuleF::F,
        _ => unreachable!(),
    }
}

#[test]
fn test_error_message_shape() {
    let grammar = Grammar::compile("f = { SOI ~ \"a\"+ ~ EOI }").unwrap();
    let err = parse_pairs(&grammar, 1, "aaab", to_rule_f).unwrap_err();
    assert_eq!(err.location, InputLocation::Pos(3));
    let display = format!("{err}");
    assert!(display.contains(" --> 1:4"), "actual display:\n{display}");
}

#[test]
fn test_rejected_stack_ops() {
    let err = Grammar::compile("x = { PUSH(\"a\") }").unwrap_err();
    assert!(err.message.contains("not supported by pika"), "actual: {err}");
}

#[test]
fn test_rejected_nullable_repetition() {
    let err = Grammar::compile("x = { (\"a\"?)* }").unwrap_err();
    assert!(err.message.contains("can match the empty string"), "actual: {err}");
}

#[test]
fn test_rejected_undefined_rule() {
    let err = Grammar::compile("x = { y }").unwrap_err();
    assert!(err.message.contains("undefined rule"), "actual: {err}");
}

#[test]
fn test_rejected_unsupported_builtin() {
    let err = Grammar::compile("x = { LETTER }").unwrap_err();
    assert!(err.message.contains("not supported by pika"), "actual: {err}");
    assert!(err.message.contains("LETTER"), "actual: {err}");
}

#[test]
fn test_rejected_tag_non_rule() {
    let err = Grammar::compile("x = { #tag = \"abc\" }").unwrap_err();
    assert!(err.message.contains("must be applied to a rule reference"), "actual: {err}");
}

#[test]
fn test_rejected_associativity_without_self_ref() {
    let err = Grammar::compile("x[1, L] = { \"a\" ~ \"b\" }").unwrap_err();
    assert!(err.message.contains("associativity requires a self-reference"), "actual: {err}");
}
