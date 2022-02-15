use std::fmt::Debug;

use {
    crate::{
        compile,
        curs::{Cursor, IntoCursor},
        dfa, nfa, parse,
        prog::Prog,
    },
    std::{cell::RefCell, sync::Arc},
};

#[derive(Clone, Debug)]
pub struct Regex<P: Copy = usize> {
    shared: Arc<Shared>,
    dfa_cache: RefCell<dfa::Cache>,
    rev_dfa_cache: RefCell<dfa::Cache>,
    nfa_cache: RefCell<nfa::Cache<P>>,
}

impl<P: Copy> Regex<P> {
    pub fn new(pattern: &str) -> Self {
        let mut parse_cache = parse::Cache::new();
        let ast = parse::parse(pattern, &mut parse_cache);
        let mut compile_cache = compile::Cache::new();
        let dfa_prog = compile::compile(
            &ast,
            compile::Options {
                dot_star: true,
                ignore_caps: true,
                byte_based: true,
                ..compile::Options::default()
            },
            &mut compile_cache,
        );
        let rev_dfa_prog = compile::compile(
            &ast,
            compile::Options {
                ignore_caps: true,
                byte_based: true,
                reversed: true,
                ..compile::Options::default()
            },
            &mut compile_cache,
        );
        let nfa_prog = compile::compile(
            &ast,
            compile::Options {
                ..compile::Options::default()
            },
            &mut compile_cache,
        );
        let dfa_cache = dfa::Cache::new(&dfa_prog);
        let rev_dfa_cache = dfa::Cache::new(&rev_dfa_prog);
        let nfa_cache = nfa::Cache::new(&nfa_prog);
        Self {
            shared: Arc::new(Shared {
                dfa_prog,
                rev_dfa_prog,
                nfa_prog,
            }),
            dfa_cache: RefCell::new(dfa_cache),
            rev_dfa_cache: RefCell::new(rev_dfa_cache),
            nfa_cache: RefCell::new(nfa_cache),
        }
    }

    pub fn run<C: IntoCursor<Pos = P>>(&self, curs: C, slots: &mut [Option<C::Pos>]) -> bool {
        let mut curs = curs.into_curs();
        let shortest_match = slots.is_empty();
        let pos = curs.pos();
        let mut dfa_cache = self.dfa_cache.borrow_mut();
        match dfa::run(
            &self.shared.dfa_prog,
            curs.by_ref(),
            dfa::Options {
                shortest_match,
                ..dfa::Options::default()
            },
            &mut *dfa_cache,
        ) {
            Ok(Some(end)) => {
                curs.set_pos(end);
                let mut rev_dfa_cache = self.rev_dfa_cache.borrow_mut();
                let start = dfa::run(
                    &self.shared.rev_dfa_prog,
                    curs.by_ref().rev(),
                    dfa::Options {
                        shortest_match,
                        ..dfa::Options::default()
                    },
                    &mut *rev_dfa_cache,
                )
                .unwrap()
                .unwrap();
                if slots.len() == 2 {
                    slots[0] = Some(start);
                    slots[1] = Some(end);
                } else if slots.len() > 2 {
                    curs.set_pos(start);
                    let mut nfa_cache = self.nfa_cache.borrow_mut();
                    nfa::run(
                        &self.shared.nfa_prog,
                        &mut curs,
                        nfa::Options::default(),
                        slots,
                        &mut *nfa_cache,
                    );
                }
                true
            }
            Ok(None) => false,
            Err(_) => {
                curs.set_pos(pos);
                let mut nfa_cache = self.nfa_cache.borrow_mut();
                nfa::run(
                    &self.shared.nfa_prog,
                    &mut curs,
                    nfa::Options {
                        shortest_match,
                        ..nfa::Options::default()
                    },
                    slots,
                    &mut *nfa_cache,
                )
            }
        }
    }
}

#[derive(Debug)]
struct Shared {
    dfa_prog: Prog,
    rev_dfa_prog: Prog,
    nfa_prog: Prog,
}
