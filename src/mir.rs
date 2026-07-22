#[derive(Debug, Clone)]
pub enum Expr {
    Constant(u16),
    Var(usize),
    Temp(usize),

    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Lt(Box<Expr>, Box<Expr>),
    Gt(Box<Expr>, Box<Expr>),
    Le(Box<Expr>, Box<Expr>),
    Ge(Box<Expr>, Box<Expr>),
    Ne(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),

    Deref(Box<Expr>),
    Concat(Vec<Expr>),
}

pub enum Mir51 {
    Assign {
        dest: Expr,
        src: Expr,
    },

    Store {
        dest_ptr: Expr,
        value: Expr,
    },

    If {
        cond: Expr,
        true_jmp: usize,
        false_jmp: usize,
    },

    Goto(usize),
    Call(usize),
    Return,
}
