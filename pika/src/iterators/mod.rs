pub mod pair;
pub mod pairs;
pub mod tokens;

pub use pair::Pair;
pub use pairs::{FlatPairs, Pairs};
pub use tokens::{Token, Tokens};

#[derive(Clone, Debug)]
pub(crate) struct PairNode<R> {
    pub(crate) rule: R,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) tag: Option<u32>,
    pub(crate) children: (u32, u32), // start and end in PairArena::child_idx
}

#[derive(Clone, Debug)]
pub(crate) struct PairArena<'i, R> {
    pub(crate) nodes: Vec<PairNode<R>>,
    pub(crate) child_idx: Vec<u32>,
    pub(crate) tags: Vec<String>,
    pub(crate) input: &'i str,
}
