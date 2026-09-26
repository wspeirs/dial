use std::rc::Rc;
use crate::{
    iterators::PairArena,
    position::Position,
    RuleType,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token<'i, R: RuleType> {
    Start { rule: R, pos: Position<'i> },
    End { rule: R, pos: Position<'i> },
}

#[derive(Clone, Debug)]
pub(crate) enum TokenStep {
    Enter(u32),
    Exit(u32),
}

#[derive(Clone)]
pub struct Tokens<'i, R: RuleType> {
    pub(crate) arena: Rc<PairArena<'i, R>>,
    pub(crate) steps: Vec<TokenStep>,
}

impl<'i, R: RuleType> Iterator for Tokens<'i, R> {
    type Item = Token<'i, R>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.steps.pop()? {
            TokenStep::Enter(idx) => {
                let n = &self.arena.nodes[idx as usize];
                self.steps.push(TokenStep::Exit(idx));
                let (start, end) = n.children;
                for &child in self.arena.child_idx[start as usize..end as usize].iter().rev() {
                    self.steps.push(TokenStep::Enter(child));
                }
                Some(Token::Start {
                    rule: n.rule,
                    pos: Position::new(self.arena.input, n.start).unwrap(),
                })
            }
            TokenStep::Exit(idx) => {
                let n = &self.arena.nodes[idx as usize];
                Some(Token::End {
                    rule: n.rule,
                    pos: Position::new(self.arena.input, n.end).unwrap(),
                })
            }
        }
    }
}
