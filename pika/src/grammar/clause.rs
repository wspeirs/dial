pub(crate) type ClauseIdx = u32;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ClauseKind {
    Nothing,
    Char(char),
    CharRange(char, char),
    Str(Box<str>),
    StrInsens(Box<str>),
    Any,
    Soi,
    Eoi,
    Seq(Box<[ClauseIdx]>),
    First(Box<[ClauseIdx]>),
    OneOrMore {
        sub: ClauseIdx,
        sep: Option<ClauseIdx>,
    },
    NotFollowedBy(ClauseIdx),
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct Clause {
    pub kind: ClauseKind,
    pub rule: Option<u32>,
    pub emit: bool,
    pub prec_group: Option<u32>,
    pub tag: Option<u32>,
    pub owner_rule: u32,
    pub can_match_zero_chars: bool,
    pub seed_parents: Box<[ClauseIdx]>,
    pub idx: ClauseIdx,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct Grammar {
    pub(crate) clauses: Vec<Clause>,
    pub(crate) terminals: Vec<ClauseIdx>,
    pub(crate) rule_names: Vec<String>,
    pub(crate) rule_entry: Vec<ClauseIdx>,
    pub(crate) tags: Vec<String>,
}

impl Grammar {
    pub fn compile(src: &str) -> Result<Grammar, crate::meta::GrammarError> {
        crate::grammar::build::compile_grammar(src)
    }

    pub fn rule_names(&self) -> &[String] {
        &self.rule_names
    }

    pub fn rule_index(&self, name: &str) -> Option<usize> {
        self.rule_names.iter().position(|r| r == name)
    }
}
