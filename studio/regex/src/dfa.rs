use {
    crate::{
        char::CharExt,
        curs::{Cursor, IntoCursor},
        leb128,
        prog::{Inst, InstPtr, Pred, Prog},
        set::Set,
    },
    std::{collections::HashMap, rc::Rc, result},
};

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug)]
pub struct Error;

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    pub shortest_match: bool,
}

#[derive(Clone, Debug)]
pub struct Cache {
    states: States,
    state_cache: HashMap<StateKey, StatePtr>,
    curr_insts: Set,
    next_insts: Set,
    add_inst_stack: Vec<InstPtr>,
}

impl Cache {
    pub fn new(prog: &Prog) -> Self {
        Self {
            states: States::new(),
            state_cache: HashMap::new(),
            curr_insts: Set::new(prog.insts.len()),
            next_insts: Set::new(prog.insts.len()),
            add_inst_stack: Vec::new(),
        }
    }
}

pub fn run<C: IntoCursor>(
    prog: &Prog,
    curs: C,
    options: Options,
    cache: &mut Cache,
) -> Result<Option<C::Pos>> {
    Dfa {
        prog,
        curs: curs.into_curs(),
        shortest_match: options.shortest_match,
        states: &mut cache.states,
        state_cache: &mut cache.state_cache,
        curr_insts: &mut cache.curr_insts,
        next_insts: &mut cache.next_insts,
        add_inst_stack: &mut cache.add_inst_stack,
    }
    .run()
}

#[derive(Debug)]
struct Dfa<'a, C> {
    prog: &'a Prog,
    curs: C,
    shortest_match: bool,
    states: &'a mut States,
    state_cache: &'a mut HashMap<StateKey, StatePtr>,
    curr_insts: &'a mut Set,
    next_insts: &'a mut Set,
    add_inst_stack: &'a mut Vec<InstPtr>,
}

impl<'a, C: Cursor> Dfa<'a, C> {
    fn run(&mut self) -> Result<Option<C::Pos>> {
        let mut matched = None;
        let start_state = self.start_state();
        let mut prev_state = start_state;
        let mut curr_state = start_state;
        let mut b = self.curs.next_byte();
        loop {
            while curr_state <= MAX_STATE && b.is_some() {
                prev_state = curr_state;
                curr_state = self.states.transitions(prev_state)[b.unwrap() as usize];
                self.curs.move_forward(1);
                b = self.curs.next_byte();
            }
            if curr_state & MATCH_STATE != 0 {
                self.curs.move_backward(1);
                matched = Some(self.curs.pos());
                self.curs.move_forward(1);
                if self.shortest_match {
                    return Ok(matched);
                }
                curr_state &= !MATCH_STATE;
                continue;
            }
            if curr_state == UNKNOWN_STATE {
                let b = self.curs.prev_byte();
                curr_state = self.next_state(prev_state, b);
                self.states.transitions_mut(prev_state)[match b {
                    Some(b) => b as usize,
                    None => 256,
                }] = curr_state;
                continue;
            } else if curr_state == ERROR_STATE {
                return Err(Error);
            }
            break;
        }
        for inst in self.states.key(curr_state).insts() {
            self.curr_insts.insert(inst);
        }
        prev_state = curr_state;
        curr_state = self.next_state(prev_state, None);
        if curr_state & MATCH_STATE != 0 {
            matched = Some(self.curs.pos());
        }
        Ok(matched)
    }

    fn start_state(&mut self) -> StatePtr {
        let prev_is_word = self
            .curs
            .prev_byte()
            .map_or(false, |b| (b as char).is_ascii_word());
        let next_is_word = self
            .curs
            .next_byte()
            .map_or(false, |b| (b as char).is_ascii_word());
        let mut flags = StateFlags::new();
        if prev_is_word {
            flags.set_word();
        }
        let preds = Preds {
            text_start: self.curs.prev_byte().is_none(),
            text_end: self.curs.next_byte().is_none(),
            word_boundary: prev_is_word == next_is_word,
        };
        AddInstView {
            prog: &self.prog,
            stack: &mut self.add_inst_stack,
        }
        .add_inst(self.curr_insts, self.prog.start, preds);
        let key = CreateStateKeyView { prog: &self.prog }
            .create_state_key(flags, self.curr_insts.as_slice());
        self.curr_insts.clear();
        self.get_or_create_state(key)
    }

    fn next_state(&mut self, state: StatePtr, b: Option<u8>) -> StatePtr {
        use std::mem;

        for inst in self.states.key(state).insts() {
            self.curr_insts.insert(inst);
        }
        if self.states.key(state).flags.assert() {
            let prev_is_word = self.states.key(state).flags.word();
            let next_is_word = b.map_or(false, |b| (b as char).is_ascii_word());
            let preds = Preds {
                text_end: b.is_none(),
                word_boundary: prev_is_word != next_is_word,
                ..Preds::new()
            };
            for &inst in self.curr_insts.as_slice() {
                AddInstView {
                    prog: &self.prog,
                    stack: &mut self.add_inst_stack,
                }
                .add_inst(self.next_insts, inst, preds);
            }
            mem::swap(&mut self.curr_insts, &mut self.next_insts);
            self.next_insts.clear();
        }
        let mut flags = StateFlags::new();
        if b.map_or(false, |b| (b as char).is_ascii_word()) {
            flags.set_word();
        }
        let preds = Preds::new();
        for &inst in self.curr_insts.as_slice() {
            match &self.prog.insts[inst] {
                Inst::Match => {
                    flags.set_matched();
                }
                Inst::ByteRange(inst_ref) => {
                    if b.map_or(false, |b| inst_ref.range.contains(&b)) {
                        AddInstView {
                            prog: &self.prog,
                            stack: &mut self.add_inst_stack,
                        }
                        .add_inst(self.next_insts, inst_ref.out, preds);
                    }
                }
                Inst::Char(_) | Inst::CharRange(_) => panic!(),
                Inst::Assert(_) => {}
                _ => panic!(),
            }
        }
        mem::swap(&mut self.curr_insts, &mut self.next_insts);
        self.next_insts.clear();
        let key = CreateStateKeyView { prog: &self.prog }
            .create_state_key(flags, self.curr_insts.as_slice());
        self.curr_insts.clear();
        let mut state = self.get_or_create_state(key);
        if flags.matched() {
            state |= MATCH_STATE;
        }
        state
    }

    fn get_or_create_state(&mut self, key: StateKey) -> StatePtr {
        use std::collections::hash_map::Entry;

        match self.state_cache.entry(key) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let key = entry.key().clone();
                let state = *entry.insert(self.states.add(key));
                if self.prog.has_word_boundary {
                    for b in 128..256 {
                        self.states.transitions_mut(state)[b] = ERROR_STATE;
                    }
                }
                state
            }
        }
    }
}

#[derive(Clone, Debug)]
struct States {
    key: Vec<StateKey>,
    transitions: Vec<StatePtr>,
}

impl States {
    fn new() -> Self {
        Self {
            key: Vec::new(),
            transitions: Vec::new(),
        }
    }

    fn key(&self, state: StatePtr) -> &StateKey {
        &self.key[state]
    }

    fn transitions(&self, state: StatePtr) -> &[StatePtr] {
        &self.transitions[257 * state..][..257]
    }

    fn transitions_mut(&mut self, state: StatePtr) -> &mut [StatePtr] {
        &mut self.transitions[257 * state..][..257]
    }

    fn add(&mut self, key: StateKey) -> StatePtr {
        use std::iter;

        let ptr = self.key.len();
        self.key.push(key);
        self.transitions
            .extend(iter::repeat(UNKNOWN_STATE).take(257));
        ptr
    }
}

type StatePtr = usize;

const UNKNOWN_STATE: StatePtr = 1 << 31;
const DEAD_STATE: StatePtr = UNKNOWN_STATE + 1;
const ERROR_STATE: StatePtr = DEAD_STATE + 1;
const MATCH_STATE: StatePtr = 1 << 30;
const MAX_STATE: StatePtr = MATCH_STATE - 1;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct StateKey {
    flags: StateFlags,
    bytes: Rc<[u8]>,
}

impl StateKey {
    fn insts(&self) -> Insts<'_> {
        Insts {
            prev_inst: 0,
            bytes: self.bytes.as_ref(),
        }
    }
}

struct Insts<'a> {
    prev_inst: InstPtr,
    bytes: &'a [u8],
}

impl<'a> Iterator for Insts<'a> {
    type Item = InstPtr;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes.is_empty() {
            return None;
        }
        let delta = leb128::read_isize(&mut self.bytes).unwrap();
        let inst = (self.prev_inst as isize + delta) as usize;
        self.prev_inst = inst as usize;
        Some(inst)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct StateFlags(u8);

impl StateFlags {
    fn new() -> Self {
        Self(0)
    }

    fn matched(&self) -> bool {
        self.0 & 1 << 0 != 0
    }

    fn set_matched(&mut self) {
        self.0 |= 1 << 0
    }

    fn assert(&self) -> bool {
        self.0 & 1 << 1 != 0
    }

    fn set_assert(&mut self) {
        self.0 |= 1 << 1;
    }

    fn word(&self) -> bool {
        self.0 & 1 << 2 != 0
    }

    fn set_word(&mut self) {
        self.0 |= 1 << 2;
    }
}

#[derive(Debug)]
struct CreateStateKeyView<'a> {
    prog: &'a Prog,
}

impl<'a> CreateStateKeyView<'a> {
    fn create_state_key(&self, flags: StateFlags, insts: &[InstPtr]) -> StateKey {
        let mut flags = flags;
        let mut bytes = Vec::new();
        let mut prev_inst = 0;
        for &inst in insts {
            match self.prog.insts[inst] {
                Inst::Assert(_) => {
                    flags.set_assert();
                }
                _ => {}
            }
            let delta = (inst as isize) - (prev_inst as isize);
            prev_inst = inst;
            leb128::write_isize(&mut bytes, delta);
        }
        StateKey {
            flags,
            bytes: Rc::from(bytes),
        }
    }
}

struct AddInstView<'a> {
    prog: &'a Prog,
    stack: &'a mut Vec<InstPtr>,
}

impl<'a> AddInstView<'a> {
    fn add_inst(&mut self, insts: &mut Set, inst: InstPtr, preds: Preds) {
        self.stack.push(inst);
        while let Some(inst) = self.stack.pop() {
            let mut inst = inst;
            loop {
                match &self.prog.insts[inst] {
                    Inst::Match | Inst::ByteRange(_) | Inst::Char(_) | Inst::CharRange(_) => {
                        insts.insert(inst);
                        break;
                    }
                    Inst::Nop(inst_ref) => {
                        inst = inst_ref.out;
                        continue;
                    }
                    Inst::Save(inst_ref) => {
                        inst = inst_ref.out;
                        continue;
                    }
                    Inst::Assert(inst_ref) => {
                        if match inst_ref.pred {
                            Pred::TextStart => preds.text_start,
                            Pred::TextEnd => preds.text_end,
                            Pred::WordBoundary => preds.word_boundary,
                            Pred::NotWordBoundary => !preds.word_boundary,
                        } {
                            inst = inst_ref.out;
                            continue;
                        }
                        insts.insert(inst);
                        break;
                    }
                    Inst::Split(inst_ref) => {
                        self.stack.push(inst_ref.out_1);
                        inst = inst_ref.out_0;
                        continue;
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Preds {
    text_start: bool,
    text_end: bool,
    word_boundary: bool,
}

impl Preds {
    fn new() -> Self {
        Self {
            text_start: false,
            text_end: false,
            word_boundary: false,
        }
    }
}
