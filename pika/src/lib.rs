pub mod error;
pub mod grammar;
pub mod iterators;
pub mod memo;
pub mod meta;
pub mod parse;
pub mod position;
pub mod span;
pub mod table;
pub mod tree;

pub use crate::{
    grammar::Grammar,
    iterators::{Pair, Pairs, Token, Tokens},
    parse::{parse_pairs, parse_table},
    position::Position,
    span::Span,
    table::ParseTable,
};

pub trait RuleType: Copy + std::fmt::Debug + Eq + std::hash::Hash + Ord {}
impl<T: Copy + std::fmt::Debug + Eq + std::hash::Hash + Ord> RuleType for T {}

pub trait Parser<R: RuleType> {
    fn parse(rule: R, input: &str) -> Result<Pairs<'_, R>, error::Error<R>>;
}
