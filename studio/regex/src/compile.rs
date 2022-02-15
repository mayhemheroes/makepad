use {
    crate::{
        ast::{Ast, Op},
        class::Class,
        prog::{Inst, InstPtr, Pred, Prog, NULL_INST},
        range::Range,
        utf8::ByteRangeSeqsCache,
    },
    std::collections::HashMap,
};

#[derive(Clone, Copy, Default)]
pub struct Options {
    pub dot_star: bool,
    pub ignore_caps: bool,
    pub byte_based: bool,
    pub reversed: bool,
}

pub fn compile(ast: &Ast, options: Options, cache: &mut Cache) -> Prog {
    let mut compiler = Compiler::new(options, cache);
    for op in ast {
        match *op {
            Op::Empty => compiler.empty(),
            Op::Cap(index) => compiler.cap(index),
            Op::Alt => compiler.alt(),
            Op::Cat => compiler.cat(),
            Op::Ques(greedy) => compiler.ques(greedy),
            Op::Star(greedy) => compiler.star(greedy),
            Op::Plus(greedy) => compiler.plus(greedy),
            Op::Assert(pred) => compiler.assert(pred),
            Op::Char(c) => compiler.char(c),
            Op::Class(ref class) => compiler.class(class),
        }
    }
    compiler.compile()
}

#[derive(Debug)]
struct Compiler<'a> {
    dot_star: bool,
    ignore_caps: bool,
    reversed: bool,
    byte_based: bool,
    emitter: Emitter,
    frag_stack: &'a mut Vec<Frag>,
    has_word_boundary: bool,
    slot_count: usize,
    byte_range_seqs_cache: &'a mut ByteRangeSeqsCache,
    class_compiler_cache: &'a mut ClassCompilerCache,
}

impl<'a> Compiler<'a> {
    fn new(options: Options, cache: &'a mut Cache) -> Self {
        let mut compiler = Self {
            dot_star: options.dot_star,
            ignore_caps: options.ignore_caps,
            byte_based: options.byte_based,
            reversed: options.reversed,
            emitter: Emitter { insts: Vec::new() },
            frag_stack: &mut cache.frag_stack,
            has_word_boundary: false,
            slot_count: 0,
            byte_range_seqs_cache: &mut cache.byte_range_seqs_cache,
            class_compiler_cache: &mut cache.class_cache,
        };
        if compiler.dot_star {
            let class = Class::from_ranges(&[Range::new('\0', char::MAX)], false);
            compiler.class(&class);
            compiler.star(false);
        }
        compiler
    }

    fn empty(&mut self) {
        let inst = self.emitter.emit(Inst::nop(NULL_INST));
        self.frag_stack
            .push(Frag::new(inst, HolePtrList::unit(HolePtr::out_0(inst))));
    }

    fn cap(&mut self, index: usize) {
        if self.ignore_caps {
            return;
        }
        let frag = self.frag_stack.pop().unwrap();
        let inst_0 = self.emitter.emit(Inst::save(frag.start, 2 * index));
        let inst_1 = self
            .emitter
            .emit(Inst::save(NULL_INST, 2 * index + 1));
        self.slot_count += 2;
        frag.ends.fill(inst_1, &mut self.emitter.insts);
        self.frag_stack
            .push(Frag::new(inst_0, HolePtrList::unit(HolePtr::out_0(inst_1))));
    }

    fn alt(&mut self) {
        let frag_1 = self.frag_stack.pop().unwrap();
        let frag_0 = self.frag_stack.pop().unwrap();
        let inst = self.emitter.emit(Inst::split(frag_0.start, frag_1.start));
        self.frag_stack.push(Frag::new(
            inst,
            frag_0.ends.concat(frag_1.ends, &mut self.emitter.insts),
        ));
    }

    fn cat(&mut self) {
        let frag_1 = self.frag_stack.pop().unwrap();
        let frag_0 = self.frag_stack.pop().unwrap();
        let frag;
        if self.reversed {
            frag_1.ends.fill(frag_0.start, &mut self.emitter.insts);
            frag = Frag::new(frag_1.start, frag_0.ends);
        } else {
            frag_0.ends.fill(frag_1.start, &mut self.emitter.insts);
            frag = Frag::new(frag_0.start, frag_1.ends);
        }
        self.frag_stack.push(frag);
    }

    fn ques(&mut self, greedy: bool) {
        let frag = self.frag_stack.pop().unwrap();
        let inst;
        let hole;
        if greedy {
            inst = self.emitter.emit(Inst::split(frag.start, NULL_INST));
            hole = HolePtr::out_1(inst);
        } else {
            inst = self.emitter.emit(Inst::split(NULL_INST, frag.start));
            hole = HolePtr::out_0(inst);
        }
        self.frag_stack.push(Frag::new(
            inst,
            frag.ends.append(hole, &mut self.emitter.insts),
        ));
    }

    fn star(&mut self, greedy: bool) {
        let frag = self.frag_stack.pop().unwrap();
        let inst;
        let hole;
        if greedy {
            inst = self.emitter.emit(Inst::split(frag.start, NULL_INST));
            hole = HolePtr::out_1(inst);
        } else {
            inst = self.emitter.emit(Inst::split(NULL_INST, frag.start));
            hole = HolePtr::out_0(inst);
        }
        frag.ends.fill(inst, &mut self.emitter.insts);
        self.frag_stack
            .push(Frag::new(inst, HolePtrList::unit(hole)));
    }

    fn plus(&mut self, greedy: bool) {
        let frag = self.frag_stack.pop().unwrap();
        let inst;
        let hole;
        if greedy {
            inst = self.emitter.emit(Inst::split(frag.start, NULL_INST));
            hole = HolePtr::out_1(inst);
        } else {
            inst = self.emitter.emit(Inst::split(NULL_INST, frag.start));
            hole = HolePtr::out_0(inst);
        }
        frag.ends.fill(inst, &mut self.emitter.insts);
        self.frag_stack
            .push(Frag::new(frag.start, HolePtrList::unit(hole)));
    }

    fn assert(&mut self, pred: Pred) {
        let inst = self.emitter.emit(Inst::assert(NULL_INST, pred));
        self.frag_stack
            .push(Frag::new(inst, HolePtrList::unit(HolePtr::out_0(inst))));
        if let Pred::WordBoundary = pred {
            self.has_word_boundary = true;
        }
    }

    fn byte_range(&mut self, range: Range<u8>) {
        let inst = self.emitter.emit(Inst::byte_range(NULL_INST, range));
        self.frag_stack
            .push(Frag::new(inst, HolePtrList::unit(HolePtr::out_0(inst))));
    }

    fn char(&mut self, c: char) {
        if self.byte_based {
            let mut bytes = [0; 4];
            let mut bytes = c.encode_utf8(&mut bytes).bytes();
            let b = bytes.next().unwrap();
            self.byte_range(Range::new(b, b));
            while let Some(b) = bytes.next() {
                self.byte_range(Range::new(b, b));
                self.cat();
            }
        } else {
            let inst = self.emitter.emit(Inst::char(NULL_INST, c));
            self.frag_stack
                .push(Frag::new(inst, HolePtrList::unit(HolePtr::out_0(inst))));
        }
    }

    fn char_range(&mut self, range: Range<char>) {
        let inst = self.emitter.emit(Inst::char_range(NULL_INST, range));
        self.frag_stack
            .push(Frag::new(inst, HolePtrList::unit(HolePtr::out_0(inst))));
    }

    fn class(&mut self, class: &Class) {
        use crate::utf8;

        if self.byte_based {
            let mut compiler = ClassCompiler::new(
                self.reversed,
                &mut self.emitter,
                &mut self.class_compiler_cache,
            );
            if self.reversed {
                for range in class {
                    for mut seq in utf8::byte_range_seqs(range, &mut self.byte_range_seqs_cache) {
                        seq.reverse();
                        compiler.add(seq.as_slice());
                    }
                }
            } else {
                for range in class {
                    for seq in utf8::byte_range_seqs(range, &mut self.byte_range_seqs_cache) {
                        compiler.add(seq.as_slice());
                    }
                }
            }
            self.frag_stack.push(compiler.compile());
        } else {
            let mut iter = class.iter();
            self.char_range(iter.next().unwrap());
            while let Some(range) = iter.next() {
                self.char_range(range);
                self.alt();
            }
        }
    }

    fn compile(mut self) -> Prog {
        if self.dot_star {
            self.reversed = false;
            self.cat();
        }
        let frag = self.frag_stack.pop().unwrap();
        let inst = self.emitter.emit(Inst::Match);
        frag.ends.fill(inst, &mut self.emitter.insts);
        Prog {
            insts: self.emitter.insts,
            start: frag.start,
            has_word_boundary: self.has_word_boundary,
            slot_count: self.slot_count,
        }
    }
}

#[derive(Debug)]
pub struct Cache {
    frag_stack: Vec<Frag>,
    byte_range_seqs_cache: ByteRangeSeqsCache,
    class_cache: ClassCompilerCache,
}

impl Cache {
    pub fn new() -> Self {
        Self {
            frag_stack: Vec::new(),
            byte_range_seqs_cache: ByteRangeSeqsCache::new(),
            class_cache: ClassCompilerCache::new(),
        }
    }
}

struct ClassCompiler<'a> {
    reversed: bool,
    emitter: &'a mut Emitter,
    compiled: &'a mut HashMap<Inst, InstPtr>,
    uncompiled: &'a mut Vec<Uncompiled>,
    ends: HolePtrList,
}

impl<'a> ClassCompiler<'a> {
    fn new(reversed: bool, emitter: &'a mut Emitter, cache: &'a mut ClassCompilerCache) -> Self {
        Self {
            reversed,
            emitter,
            compiled: &mut cache.compiled,
            uncompiled: &mut cache.uncompiled,
            ends: HolePtrList::empty(),
        }
    }

    fn add(&mut self, ranges: &[Range<u8>]) {
        let prefix_len = self.prefix_len(ranges);
        let inst = self.compile_suffix(prefix_len);
        self.append_suffix(inst, &ranges[prefix_len..]);
    }

    fn prefix_len(&mut self, ranges: &[Range<u8>]) -> usize {
        if self.reversed {
            0
        } else {
            ranges
                .iter()
                .zip(self.uncompiled.iter())
                .take_while(|&(&range, uncompiled)| range == uncompiled.range)
                .count()
        }
    }

    fn compile_suffix(&mut self, start: usize) -> InstPtr {
        use std::mem;

        let mut inst = NULL_INST;
        while self.uncompiled.len() > start {
            let uncompiled = self.uncompiled.pop().unwrap();
            let has_hole = inst == NULL_INST;
            let (next_inst, is_new) = self.get_or_emit(Inst::byte_range(inst, uncompiled.range));
            inst = next_inst;
            if is_new && has_hole {
                let ends = mem::replace(&mut self.ends, HolePtrList::empty());
                self.ends = ends.append(HolePtr::out_0(inst), &mut self.emitter.insts);
            }
            if uncompiled.inst != NULL_INST {
                let (next_inst, _) = self.get_or_emit(Inst::split(uncompiled.inst, inst));
                inst = next_inst;
            }
        }
        inst
    }

    fn append_suffix(&mut self, inst: InstPtr, ranges: &[Range<u8>]) {
        self.uncompiled.push(Uncompiled {
            inst,
            range: ranges[0],
        });
        for &range in &ranges[1..] {
            self.uncompiled.push(Uncompiled {
                inst: NULL_INST,
                range,
            });
        }
    }

    fn get_or_emit(&mut self, inst: Inst) -> (InstPtr, bool) {
        use std::collections::hash_map::Entry;

        match self.compiled.entry(inst) {
            Entry::Occupied(entry) => (*entry.get(), false),
            Entry::Vacant(entry) => (*entry.insert(self.emitter.emit(inst)), true),
        }
    }

    fn compile(mut self) -> Frag {
        let start = self.compile_suffix(0);
        self.compiled.clear();
        Frag::new(start, self.ends)
    }
}

#[derive(Debug)]
struct ClassCompilerCache {
    compiled: HashMap<Inst, InstPtr>,
    uncompiled: Vec<Uncompiled>,
}

impl ClassCompilerCache {
    fn new() -> Self {
        Self {
            compiled: HashMap::new(),
            uncompiled: Vec::new(),
        }
    }
}

#[derive(Debug)]
struct Uncompiled {
    inst: InstPtr,
    range: Range<u8>,
}

impl Uncompiled {
    fn new(inst: InstPtr, range: Range<u8>) -> Self {
        Self { inst, range }
    }
}

#[derive(Debug)]
struct Emitter {
    insts: Vec<Inst>,
}

impl Emitter {
    fn emit(&mut self, inst: Inst) -> InstPtr {
        let ptr = self.insts.len();
        self.insts.push(inst);
        ptr
    }
}

#[derive(Debug)]
struct Frag {
    start: InstPtr,
    ends: HolePtrList,
}

impl Frag {
    fn new(start: InstPtr, ends: HolePtrList) -> Self {
        Self { start, ends }
    }
}

#[derive(Debug)]
struct HolePtrList {
    head: HolePtr,
    tail: HolePtr,
}

impl HolePtrList {
    fn empty() -> Self {
        Self {
            head: HolePtr::null(),
            tail: HolePtr::null(),
        }
    }

    fn unit(hole: HolePtr) -> Self {
        Self {
            head: hole,
            tail: hole,
        }
    }

    fn append(self, hole: HolePtr, insts: &mut [Inst]) -> Self {
        self.concat(Self::unit(hole), insts)
    }

    fn concat(self, other: Self, insts: &mut [Inst]) -> Self {
        if self.tail.is_null() {
            return other;
        }
        if self.head.is_null() {
            return self;
        }
        *self.tail.get_mut(insts) = other.head.0;
        Self {
            head: self.head,
            tail: other.tail,
        }
    }

    fn fill(self, inst: InstPtr, insts: &mut [Inst]) {
        let mut curr = self.head;
        while curr.0 != NULL_INST {
            let next = *curr.get(insts);
            *curr.get_mut(insts) = inst;
            curr = HolePtr(next);
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct HolePtr(usize);

impl HolePtr {
    fn null() -> Self {
        Self(NULL_INST)
    }

    fn out_0(inst: InstPtr) -> Self {
        Self(inst << 1)
    }

    fn out_1(inst: InstPtr) -> Self {
        Self(inst << 1 | 1)
    }

    fn is_null(self) -> bool {
        self.0 == NULL_INST
    }

    fn get(self, insts: &[Inst]) -> &InstPtr {
        let inst_ref = &insts[self.0 >> 1];
        if self.0 & 1 == 0 {
            inst_ref.out_0()
        } else {
            inst_ref.out_1()
        }
    }

    fn get_mut(self, insts: &mut [Inst]) -> &mut InstPtr {
        let inst_ref = &mut insts[self.0 >> 1];
        if self.0 & 1 == 0 {
            inst_ref.out_0_mut()
        } else {
            inst_ref.out_1_mut()
        }
    }
}
