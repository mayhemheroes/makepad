use {
    crate::{ast, ast::Ast, class, class::Class, prog::Pred, range::Range, unicode},
    std::{mem, str::Chars},
};

pub fn parse(pattern: &str, cache: &mut Cache) -> Ast {
    let mut chars = pattern.chars();
    Parser {
        c0: chars.next(),
        c1: chars.next(),
        chars,
        pos: 0,
        next_cap_index: 1,
        expr_stack: &mut cache.expr_stack,
        expr: Expr::new(Some(0)),
        class_builder: &mut cache.class_builder,
        ast_builder: &mut cache.ast_builder,
    }
    .parse()
}

#[derive(Debug)]
struct Parser<'a> {
    c0: Option<char>,
    c1: Option<char>,
    chars: Chars<'a>,
    pos: usize,
    next_cap_index: usize,
    expr_stack: &'a mut Vec<Expr>,
    expr: Expr,
    class_builder: &'a mut class::Builder,
    ast_builder: &'a mut ast::Builder,
}

impl<'a> Parser<'a> {
    fn parse(&mut self) -> Ast {
        loop {
            match self.peek_2() {
                (Some('('), _) => {
                    self.skip();
                    let mut cap = true;
                    if self.peek_2() == (Some('?'), Some(':')) {
                        self.skip_2();
                        cap = false;
                    }
                    let cap_index = if cap {
                        let cap_index = self.next_cap_index;
                        self.next_cap_index += 1;
                        Some(cap_index)
                    } else {
                        None
                    };
                    self.push(cap_index);
                }
                (Some(')'), _) => {
                    self.skip();
                    self.pop();
                }
                (Some('|'), _) => {
                    self.skip();
                    self.alt();
                }
                (Some('?'), _) => {
                    self.skip();
                    let mut greedy = true;
                    if self.peek() == Some('?') {
                        self.skip();
                        greedy = false;
                    }
                    self.ast_builder.ques(greedy);
                }
                (Some('*'), _) => {
                    self.skip();
                    let mut greedy = true;
                    if self.peek() == Some('?') {
                        self.skip();
                        greedy = false;
                    }
                    self.ast_builder.star(greedy);
                }
                (Some('+'), _) => {
                    self.skip();
                    let mut greedy = true;
                    if self.peek() == Some('?') {
                        self.skip();
                        greedy = false;
                    }
                    self.ast_builder.plus(greedy);
                }
                (Some('{'), _) => {
                    self.skip();
                    let min = self.parse_dec_digits();
                    let max = if self.peek() == Some(',') {
                        self.skip();
                        if self.peek() == Some('}') {
                            None
                        } else {
                            Some(self.parse_dec_digits())
                        }
                    } else {
                        Some(min)
                    };
                    if self.peek() != Some('}') {
                        panic!()
                    }
                    self.skip();
                    let mut greedy = true;
                    if self.peek() == Some('?') {
                        self.skip();
                        greedy = false;
                    }
                    self.ast_builder.rep(min, max, greedy);
                }
                (Some('['), _) => {
                    self.skip();
                    let mut negated = false;
                    if self.peek() == Some('^') {
                        self.skip();
                        negated = true;
                    }
                    loop {
                        match self.peek_2() {
                            (Some(']'), _) => {
                                self.skip();
                                break;
                            }
                            (Some('\\'), Some('D')) => {
                                self.skip_2();
                                self.class_builder.insert_ranges(&unicode::DIGIT, true);
                            }
                            (Some('\\'), Some('S')) => {
                                self.skip_2();
                                self.class_builder.insert_ranges(&unicode::SPACE, true);
                            }
                            (Some('\\'), Some('W')) => {
                                self.skip_2();
                                self.class_builder.insert_ranges(&unicode::WORD, true);
                            }
                            (Some('\\'), Some('d')) => {
                                self.skip_2();
                                self.class_builder.insert_ranges(&unicode::DIGIT, false);
                            }
                            (Some('\\'), Some('s')) => {
                                self.skip_2();
                                self.class_builder.insert_ranges(&unicode::SPACE, false);
                            }
                            (Some('\\'), Some('w')) => {
                                self.skip_2();
                                self.class_builder.insert_ranges(&unicode::WORD, false);
                            }
                            (Some(c), _) => {
                                self.skip();
                                self.class_builder.insert_ranges(&[Range::new(c, c)], false);
                            }
                            (None, _) => panic!(),
                        }
                    }
                    let class = self.class_builder.build(negated);
                    self.class(class);
                }
                (Some('\\'), Some('B')) => {
                    self.skip_2();
                    self.assert(Pred::NotWordBoundary);
                }
                (Some('\\'), Some('D')) => {
                    self.skip_2();
                    self.class(Class::from_ranges(&unicode::DIGIT, true));
                }
                (Some('\\'), Some('S')) => {
                    self.skip_2();
                    self.class(Class::from_ranges(&unicode::SPACE, true));
                }
                (Some('\\'), Some('W')) => {
                    self.skip_2();
                    self.class(Class::from_ranges(&unicode::WORD, true));
                }
                (Some('\\'), Some('b')) => {
                    self.skip_2();
                    self.assert(Pred::WordBoundary);
                }
                (Some('\\'), Some('d')) => {
                    self.skip_2();
                    self.class(Class::from_ranges(&unicode::DIGIT, false));
                }
                (Some('\\'), Some('s')) => {
                    self.skip_2();
                    self.class(Class::from_ranges(&unicode::SPACE, false));
                }
                (Some('\\'), Some('w')) => {
                    self.skip_2();
                    self.class(Class::from_ranges(&unicode::WORD, false));
                }
                (Some(c), _) => {
                    self.skip();
                    self.char(c);
                }
                (None, _) => break,
            }
        }
        self.alt();
        if self.expr.term_count == 0 {
            self.ast_builder.empty();
        }
        self.ast_builder.cap(self.expr.cap_index.unwrap());
        self.ast_builder.build()
    }

    fn parse_dec_digits(&mut self) -> u32 {
        let c = match self.peek() {
            Some(c) if c.is_digit(10) => c,
            _ => panic!(),
        };
        self.skip();
        let mut value = c.to_digit(10).unwrap();
        loop {
            let c = match self.peek() {
                Some(c) if c.is_digit(10) => c,
                _ => break,
            };
            self.skip();
            value = 10 * value + c.to_digit(10).unwrap();
        }
        value
    }

    fn peek(&self) -> Option<char> {
        self.c0
    }

    fn peek_2(&self) -> (Option<char>, Option<char>) {
        (self.c0, self.c1)
    }

    fn skip(&mut self) {
        self.pos += self.c0.map_or(0, |c0| c0.len_utf8());
        self.c0 = self.c1;
        self.c1 = self.chars.next();
    }

    fn skip_2(&mut self) {
        self.pos += self.c0.map_or(0, |c0| c0.len_utf8());
        self.pos += self.c1.map_or(0, |c1| c1.len_utf8());
        self.c0 = self.chars.next();
        self.c1 = self.chars.next();
    }

    fn push(&mut self, cap_index: Option<usize>) {
        self.cat();
        let expr = mem::replace(&mut self.expr, Expr::new(cap_index));
        self.expr_stack.push(expr);
    }

    fn pop(&mut self) {
        self.alt();
        if self.expr.term_count == 0 {
            self.ast_builder.empty();
        }
        if let Some(index) = self.expr.cap_index {
            self.ast_builder.cap(index);
        }
        self.expr = self.expr_stack.pop().unwrap();
        self.expr.fact_count += 1;
    }

    fn alt(&mut self) {
        self.cat();
        if self.expr.fact_count != 0 {
            self.expr.term_count += 1;
            self.expr.fact_count = 0;
        }
        if self.expr.term_count == 2 {
            self.ast_builder.alt();
            self.expr.term_count -= 1;
        }
    }

    fn cat(&mut self) {
        if self.expr.fact_count == 2 {
            self.ast_builder.cat();
            self.expr.fact_count -= 1;
        }
    }

    fn assert(&mut self, pred: Pred) {
        self.cat();
        self.expr.fact_count += 1;
        self.ast_builder.assert(pred);
    }

    fn char(&mut self, c: char) {
        self.cat();
        self.expr.fact_count += 1;
        self.ast_builder.char(c);
    }

    fn class(&mut self, class: Class) {
        self.cat();
        self.expr.fact_count += 1;
        self.ast_builder.class(class);
    }
}

#[derive(Debug)]
pub struct Cache {
    expr_stack: Vec<Expr>,
    class_builder: class::Builder,
    ast_builder: ast::Builder,
}

impl Cache {
    pub fn new() -> Self {
        Self {
            expr_stack: Vec::new(),
            class_builder: class::Builder::new(),
            ast_builder: ast::Builder::new(),
        }
    }
}

#[derive(Debug)]
struct Expr {
    cap_index: Option<usize>,
    term_count: usize,
    fact_count: usize,
}

impl Expr {
    fn new(cap_index: Option<usize>) -> Self {
        Self {
            cap_index,
            term_count: 0,
            fact_count: 0,
        }
    }
}
