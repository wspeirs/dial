use std::rc::Rc;
use crate::{
    iterators::{
        tokens::{TokenStep, Tokens},
        PairArena, Pairs,
    },
    span::Span,
    RuleType,
};

#[derive(Clone)]
pub struct Pair<'i, R: RuleType> {
    pub(crate) arena: Rc<PairArena<'i, R>>,
    pub(crate) node: u32,
}

impl<'i, R: RuleType> Pair<'i, R> {
    #[inline]
    pub fn as_rule(&self) -> R {
        self.arena.nodes[self.node as usize].rule
    }

    #[inline]
    pub fn as_str(&self) -> &'i str {
        let n = &self.arena.nodes[self.node as usize];
        &self.arena.input[n.start..n.end]
    }

    #[inline]
    pub fn get_input(&self) -> &'i str {
        self.arena.input
    }

    pub fn as_span(&self) -> Span<'i> {
        let n = &self.arena.nodes[self.node as usize];
        Span {
            input: self.arena.input,
            start: n.start,
            end: n.end,
        }
    }

    pub fn as_node_tag(&self) -> Option<&str> {
        let tag_id = self.arena.nodes[self.node as usize].tag?;
        Some(&self.arena.tags[tag_id as usize])
    }

    pub fn into_inner(self) -> Pairs<'i, R> {
        let children = self.arena.nodes[self.node as usize].children;
        Pairs {
            arena: self.arena,
            range: children,
        }
    }
    pub fn tokens(self) -> Tokens<'i, R> {
        Tokens {
            arena: self.arena,
            steps: vec![TokenStep::Enter(self.node)],
        }
    }

    pub fn line_col(&self) -> (usize, usize) {
        self.as_span().start_pos().line_col()
    }
}

impl<'i, R: RuleType> PartialEq for Pair<'i, R> {
    fn eq(&self, other: &Self) -> bool {
        self.as_rule() == other.as_rule()
            && self.as_span() == other.as_span()
            && self.clone().into_inner() == other.clone().into_inner()
    }
}

impl<'i, R: RuleType> Eq for Pair<'i, R> {}

impl<'i, R: RuleType> std::hash::Hash for Pair<'i, R> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_rule().hash(state);
        self.as_span().hash(state);
    }
}

impl<'i, R: RuleType> std::fmt::Display for Pair<'i, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rule = self.as_rule();
        let span = self.as_span();
        let inner: Vec<_> = self.clone().into_inner().collect();
        if inner.is_empty() {
            write!(f, "{:?}({}, {})", rule, span.start(), span.end())
        } else {
            write!(f, "{:?}({}, {}, [", rule, span.start(), span.end())?;
            for (i, child) in inner.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", child)?;
            }
            write!(f, "])")
        }
    }
}

impl<'i, R: RuleType> std::fmt::Debug for Pair<'i, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pair")
            .field("rule", &self.as_rule())
            .field("span", &self.as_span())
            .field("inner", &self.clone().into_inner())
            .finish()
    }
}
