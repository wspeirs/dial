use pika::meta::parse_grammar;

#[test]
fn test_parse_dial_grammar() {
    let grammar_src = include_str!("../../src/grammar.pika");
    let grammar = parse_grammar(grammar_src).expect("should parse grammar.pest");
    assert_eq!(grammar.rules.len(), 75);
    assert_eq!(grammar.rules[0].name, "WHITESPACE");
    assert_eq!(grammar.rules[1].name, "COMMENT");
    let names: Vec<_> = grammar.rules.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"file"));
    assert!(names.contains(&"main"));
    assert!(names.contains(&"blockless_expression"));
    assert!(names.contains(&"WHITESPACE"));
    assert!(names.contains(&"COMMENT"));
}

#[test]
fn test_parse_precedence_and_tags() {
    let src = r#"
E[4] = { "(" ~ E ~ ")" }
E[3] = { ASCII_DIGIT+ | ASCII_ALPHA_LOWER+ }
E[2] = { "-" ~ E }
E[1, L] = { E ~ ("*" | "/") ~ E }
E[0, R] = { #tag = E ~ ("+" | "-") ~ E }
"#;
    let grammar = parse_grammar(src).expect("should parse precedence and tags");
    assert_eq!(grammar.rules.len(), 5);
}

#[test]
fn test_meta_error_reporting() {
    let err = parse_grammar("r = { \"\\q\" }").unwrap_err();
    assert!(err.message.contains("invalid escape sequence"));
    let display = format!("{err}");
    assert!(display.contains(" --> 1:"));
    assert!(display.contains(" |"));
}
