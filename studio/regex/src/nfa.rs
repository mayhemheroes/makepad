use crate::{
    curs::{Cursor, IntoCursor},
    prog::{Inst, InstPtr, Pred, Prog},
    set::Set,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    pub shortest_match: bool,
}

#[derive(Clone, Debug)]
pub struct Cache<P> {
    curr_threads: Threads<P>,
    next_threads: Threads<P>,
    add_thread_stack: Vec<AddThreadFrame<P>>,
}

impl<P> Cache<P> {
    pub fn new(prog: &Prog) -> Self {
        Self {
            curr_threads: Threads::new(prog.insts.len(), prog.slot_count),
            next_threads: Threads::new(prog.insts.len(), prog.slot_count),
            add_thread_stack: Vec::new(),
        }
    }
}

pub fn run<C: IntoCursor>(
    prog: &Prog,
    curs: C,
    options: Options,
    slots: &mut [Option<C::Pos>],
    cache: &mut Cache<C::Pos>,
) -> bool {
    Nfa {
        prog,
        curs: curs.into_curs(),
        shortest_match: options.shortest_match,
        curr_threads: &mut cache.curr_threads,
        next_threads: &mut cache.next_threads,
        add_thread_stack: &mut cache.add_thread_stack,
    }
    .run(slots)
}

#[derive(Debug)]
struct Nfa<'a, C: Cursor> {
    prog: &'a Prog,
    curs: C,
    shortest_match: bool,
    curr_threads: &'a mut Threads<C::Pos>,
    next_threads: &'a mut Threads<C::Pos>,
    add_thread_stack: &'a mut Vec<AddThreadFrame<C::Pos>>,
}

impl<'a, C: Cursor> Nfa<'a, C> {
    fn run(&mut self, slots: &mut [Option<C::Pos>]) -> bool {
        use std::mem;

        let mut matched = false;
        loop {
            AddThreadView {
                prog: &self.prog,
                curs: &self.curs,
                stack: &mut self.add_thread_stack,
            }.add_thread(&mut self.next_threads, self.prog.start, slots);
            mem::swap(&mut self.curr_threads, &mut self.next_threads);
            self.next_threads.inst.clear();
            let c = self.curs.next_char();
            if let Some(c) = c {
                self.curs.move_forward(c.len_utf8());
            }
            let mut view = AddThreadView {
                prog: &self.prog,
                curs: &self.curs,
                stack: &mut self.add_thread_stack,
            };
            for &inst in self.curr_threads.inst.as_slice() {
                match &self.prog.insts[inst] {
                    Inst::Match => {
                        slots.copy_from_slice(self.curr_threads.slots.get(inst));
                        if self.shortest_match {
                            return true;
                        }
                        matched = true;
                        break;
                    }
                    Inst::ByteRange(_) => panic!(),
                    Inst::Char(inst_ref) => {
                        if c.map_or(false, |c| c == inst_ref.c) {
                            view.add_thread(
                                &mut self.next_threads,
                                inst_ref.out,
                                self.curr_threads.slots.get_mut(inst),
                            );
                        }
                    }
                    Inst::CharRange(inst_ref) => {
                        if c.map_or(false, |c| inst_ref.range.contains(&c)) {
                            view.add_thread(
                                &mut self.next_threads,
                                inst_ref.out,
                                self.curr_threads.slots.get_mut(inst),
                            );
                        }
                    }
                    _ => panic!(),
                }
            }
            if c.is_none() {
                break;
            }
        }
        matched
    }
}

#[derive(Clone, Debug)]
struct Threads<P> {
    inst: Set,
    slots: Slots<P>,
}

impl<P> Threads<P> {
    fn new(inst_count: usize, slot_count: usize) -> Self {
        Self {
            inst: Set::new(inst_count),
            slots: Slots {
                slots: (0..inst_count * slot_count).map(|_| None).collect(),
                slot_count,
            },
        }
    }
}

#[derive(Clone, Debug)]
struct Slots<P> {
    slots: Vec<Option<P>>,
    slot_count: usize,
}

impl<P> Slots<P> {
    fn get(&self, inst: InstPtr) -> &[Option<P>] {
        &self.slots[inst * self.slot_count..][..self.slot_count]
    }

    fn get_mut(&mut self, inst: InstPtr) -> &mut [Option<P>] {
        &mut self.slots[inst * self.slot_count..][..self.slot_count]
    }
}

#[derive(Clone, Debug)]
enum AddThreadFrame<P> {
    AddThread(InstPtr),
    UnsaveSlots(usize, Option<P>),
}

#[derive(Debug)]
struct AddThreadView<'a, C: Cursor> {
    prog: &'a Prog,
    curs: &'a C,
    stack: &'a mut Vec<AddThreadFrame<C::Pos>>,
}

impl<'a, C: Cursor> AddThreadView<'a, C> {
    fn add_thread(
        &mut self,
        threads: &mut Threads<C::Pos>,
        inst: InstPtr,
        slots: &mut [Option<C::Pos>],
    ) {
        self.stack.push(AddThreadFrame::AddThread(inst));
        while let Some(frame) = self.stack.pop() {
            match frame {
                AddThreadFrame::AddThread(inst) => {
                    let mut inst = inst;
                    loop {
                        match &self.prog.insts[inst] {
                            Inst::Match
                            | Inst::ByteRange(_)
                            | Inst::Char(_)
                            | Inst::CharRange(_) => {
                                if threads.inst.insert(inst) {
                                    threads.slots.get_mut(inst).copy_from_slice(slots);
                                }
                                break;
                            }
                            Inst::Nop(inst_ref) => {
                                inst = inst_ref.out;
                                continue;
                            }
                            Inst::Save(inst_ref) => {
                                self.stack.push(AddThreadFrame::UnsaveSlots(
                                    inst_ref.slot_index,
                                    slots[inst_ref.slot_index],
                                ));
                                slots[inst_ref.slot_index] = Some(self.curs.pos());
                                inst = inst_ref.out;
                                continue;
                            }
                            Inst::Assert(inst_ref) => {
                                if match inst_ref.pred {
                                    Pred::TextStart => self.curs.text_start(),
                                    Pred::TextEnd => self.curs.text_end(),
                                    Pred::WordBoundary => self.curs.word_boundary(),
                                    Pred::NotWordBoundary => !self.curs.word_boundary(),
                                } {
                                    inst = inst_ref.out;
                                    continue;
                                }
                                break;
                            }
                            Inst::Split(inst_ref) => {
                                self.stack.push(AddThreadFrame::AddThread(inst_ref.out_1));
                                inst = inst_ref.out_0;
                                continue;
                            }
                        }
                    }
                }
                AddThreadFrame::UnsaveSlots(index, old_pos) => slots[index] = old_pos,
            }
        }
    }
}
