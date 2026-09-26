use std::rc::Rc;
use crate::{
    iterators::{
        pair::Pair,
        tokens::{TokenStep, Tokens},
        PairArena,
    },
    RuleType,
};

#[derive(Clone)]
pub struct Pairs<'i, R: RuleType> {
    pub(crate) arena: Rc<PairArena<'i, R>>,
    pub(crate) range: (u32, u32),
}

impl<'i, R: RuleType> Pairs<'i, R> {
    pub fn single(pair: Pair<'i, R>) -> Self {
        let mut child_idx = pair.arena.child_idx.clone();
        let start = child_idx.len() as u32;
        child_idx.push(pair.node);
        let arena = Rc::new(PairArena {
            nodes: pair.arena.nodes.clone(),
            child_idx,
            tags: pair.arena.tags.clone(),
            input: pair.arena.input,
        });
        Pairs {
            arena,
            range: (start, start + 1),
        }
    }

    pub fn as_str(&self) -> &'i str {
        if self.range.0 == self.range.1 {
            ""
        } else {
            let first_idx = self.arena.child_idx[self.range.0 as usize] as usize;
            let last_idx = self.arena.child_idx[(self.range.1 - 1) as usize] as usize;
            let start = self.arena.nodes[first_idx].start;
            let end = self.arena.nodes[last_idx].end;
            &self.arena.input[start..end]
        }
    }

    #[inline]
    pub fn get_input(&self) -> &'i str {
        self.arena.input
    }

    pub fn concat(&self) -> String {
        self.as_str().to_string()
    }

    pub fn flatten(self) -> FlatPairs<'i, R> {
        let mut stack = Vec::new();
        for &child in self.arena.child_idx[self.range.0 as usize..self.range.1 as usize]
            .iter()
            .rev()
        {
            stack.push(child);
        }
        FlatPairs {
            arena: self.arena,
            stack,
        }
    }

    pub fn tokens(self) -> Tokens<'i, R> {
        let mut steps = Vec::new();
        for &child in self.arena.child_idx[self.range.0 as usize..self.range.1 as usize]
            .iter()
            .rev()
        {
            steps.push(TokenStep::Enter(child));
        }
        Tokens {
            arena: self.arena,
            steps,
        }
    }

    pub fn find_first_tagged(&self, tag: &str) -> Option<Pair<'i, R>> {
        for pair in self.clone().flatten() {
            if pair.as_node_tag() == Some(tag) {
                return Some(pair);
            }
        }
        None
    }

    pub fn find_tagged<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = Pair<'i, R>> + 'a {
        self.clone().flatten().filter(move |p| p.as_node_tag() == Some(tag))
    }
}

impl<'i, R: RuleType> Iterator for Pairs<'i, R> {
    type Item = Pair<'i, R>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.range.0 < self.range.1 {
            let idx = self.arena.child_idx[self.range.0 as usize];
            self.range.0 += 1;
            Some(Pair {
                arena: self.arena.clone(),
                node: idx,
            })
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = (self.range.1 - self.range.0) as usize;
        (len, Some(len))
    }
}

impl<'i, R: RuleType> DoubleEndedIterator for Pairs<'i, R> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.range.0 < self.range.1 {
            self.range.1 -= 1;
            let idx = self.arena.child_idx[self.range.1 as usize];
            Some(Pair {
                arena: self.arena.clone(),
                node: idx,
            })
        } else {
            None
        }
    }
}

impl<'i, R: RuleType> ExactSizeIterator for Pairs<'i, R> {}

impl<'i, R: RuleType> PartialEq for Pairs<'i, R> {
    fn eq(&self, other: &Self) -> bool {
        let left: Vec<_> = self.clone().collect();
        let right: Vec<_> = other.clone().collect();
        left == right
    }
}

impl<'i, R: RuleType> Eq for Pairs<'i, R> {}

impl<'i, R: RuleType> std::fmt::Debug for Pairs<'i, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let pairs: Vec<_> = self.clone().collect();
        f.debug_list().entries(pairs.iter()).finish()
    }
}

#[derive(Clone)]
pub struct FlatPairs<'i, R: RuleType> {
    arena: Rc<PairArena<'i, R>>,
    stack: Vec<u32>,
}

impl<'i, R: RuleType> Iterator for FlatPairs<'i, R> {
    type Item = Pair<'i, R>;

    fn next(&mut self) -> Option<Self::Item> {
        let node_idx = self.stack.pop()?;
        let node = &self.arena.nodes[node_idx as usize];
        let (start, end) = node.children;
        for &child in self.arena.child_idx[start as usize..end as usize].iter().rev() {
            self.stack.push(child);
        }
        Some(Pair {
            arena: self.arena.clone(),
            node: node_idx,
        })
    }
}
