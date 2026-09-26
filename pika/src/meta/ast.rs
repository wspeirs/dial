#[derive(Clone, Debug, PartialEq)]
pub struct Grammar {
    pub rules: Vec<Rule>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub name: String,
    pub prec: Option<Prec>,
    pub ty: RuleTy,
    pub expr: Expr,
    pub span: (usize, usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Prec {
    pub level: u32,
    pub assoc: Option<Assoc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Assoc {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleTy {
    Normal,
    Silent,
    Atomic,
    CompoundAtomic,
    NonAtomic,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Str(String),
    Insens(String),
    Range(char, char),
    Ident(String, (usize, usize)),
    Seq(Vec<Expr>),
    Choice(Vec<Expr>),
    Opt(Box<Expr>),
    Rep(Box<Expr>),
    RepOnce(Box<Expr>),
    RepExact(Box<Expr>, u32),
    RepMin(Box<Expr>, u32),
    RepMax(Box<Expr>, u32),
    RepMinMax(Box<Expr>, u32, u32),
    PosPred(Box<Expr>),
    NegPred(Box<Expr>),
    Tag(Box<Expr>, String),
}
