use crate::range::Range;

pub const MAX_LEN: usize = 4;

#[derive(Debug)]
pub struct ByteRangeSeqs<'a> {
    range_stack: &'a mut Vec<Range<u32>>,
}

impl<'a> Iterator for ByteRangeSeqs<'a> {
    type Item = ByteRangeSeq;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(mut range) = self.range_stack.pop() {
            'LOOP: loop {
                if range.end <= 0x7F {
                    return Some(ByteRangeSeq::One([Range::new(
                        range.start as u8,
                        range.end as u8,
                    )]));
                }
                if range.start < 0xE000 && range.end > 0xD7FF {
                    self.range_stack.push(Range::new(0xE000, range.end));
                    range.end = 0xD7FF;
                    continue 'LOOP;
                }
                for index in 1..MAX_LEN {
                    let max = max_scalar(index);
                    if range.start <= max && max < range.end {
                        self.range_stack.push(Range::new(max + 1, range.end));
                        range.end = max;
                        continue 'LOOP;
                    }
                }
                for index in 1..MAX_LEN {
                    let mask = (1 << (6 * index)) - 1;
                    if range.start & !mask != range.end & !mask {
                        if range.start & mask != 0 {
                            self.range_stack
                                .push(Range::new((range.start | mask) + 1, range.end));
                            range.end = range.start | mask;
                            continue 'LOOP;
                        }
                        if range.end & mask != mask {
                            self.range_stack
                                .push(Range::new(range.end & !mask, range.end));
                            range.end = (range.end & !mask) - 1;
                            continue 'LOOP;
                        }
                    }
                }
                let mut start = [0; MAX_LEN];
                let start = char::from_u32(range.start)
                    .unwrap()
                    .encode_utf8(&mut start)
                    .as_bytes();
                let mut end = [0; MAX_LEN];
                let end = char::from_u32(range.end)
                    .unwrap()
                    .encode_utf8(&mut end)
                    .as_bytes();
                assert_eq!(start.len(), end.len());
                return Some(match start.len() {
                    2 => ByteRangeSeq::Two([
                        Range::new(start[0], end[0]),
                        Range::new(start[1], end[1]),
                    ]),
                    3 => ByteRangeSeq::Three([
                        Range::new(start[0], end[0]),
                        Range::new(start[1], end[1]),
                        Range::new(start[2], end[2]),
                    ]),
                    4 => ByteRangeSeq::Four([
                        Range::new(start[0], end[0]),
                        Range::new(start[1], end[1]),
                        Range::new(start[2], end[2]),
                        Range::new(start[3], end[3]),
                    ]),
                    _ => panic!(),
                });
            }
        }
        None
    }
}

#[derive(Debug)]
pub struct ByteRangeSeqsCache {
    range_stack: Vec<Range<u32>>,
}

impl ByteRangeSeqsCache {
    pub fn new() -> Self {
        Self {
            range_stack: Vec::new(),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum ByteRangeSeq {
    One([Range<u8>; 1]),
    Two([Range<u8>; 2]),
    Three([Range<u8>; 3]),
    Four([Range<u8>; 4]),
}

impl ByteRangeSeq {
    pub fn new(ranges: &[Range<u8>]) -> Self {
        match ranges.len() {
            1 => ByteRangeSeq::One([ranges[0]]),
            2 => ByteRangeSeq::Two([ranges[0], ranges[1]]),
            3 => ByteRangeSeq::Three([ranges[0], ranges[1], ranges[2]]),
            4 => ByteRangeSeq::Four([ranges[0], ranges[1], ranges[2], ranges[3]]),
            _ => panic!(),
        }
    }

    pub fn as_slice(&self) -> &[Range<u8>] {
        match self {
            Self::One(seq) => seq.as_slice(),
            Self::Two(seq) => seq.as_slice(),
            Self::Three(seq) => seq.as_slice(),
            Self::Four(seq) => seq.as_slice(),
        }
    }

    pub fn reverse(&mut self) {
        match self {
            Self::One(_) => {}
            Self::Two(seq) => seq.reverse(),
            Self::Three(seq) => seq.reverse(),
            Self::Four(seq) => seq.reverse(),
        }
    }
}

pub fn byte_range_seqs(range: Range<char>, cache: &mut ByteRangeSeqsCache) -> ByteRangeSeqs<'_> {
    let range_stack = &mut cache.range_stack;
    range_stack.clear();
    range_stack.push(Range::new(range.start as u32, range.end as u32));
    ByteRangeSeqs { range_stack }
}

fn max_scalar(len: usize) -> u32 {
    match len {
        1 => 0x7F,
        2 => 0x7FF,
        3 => 0xFFFF,
        4 => 0x10FFFF,
        _ => panic!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all() {
        let mut cache = ByteRangeSeqsCache::new();
        assert_eq!(
            super::byte_range_seqs(Range::new('\u{0}', '\u{10FFFF}'), &mut cache)
                .collect::<Vec<_>>(),
            vec![
                ByteRangeSeq::One([Range::new(0x00, 0x7F)]),
                ByteRangeSeq::Two([Range::new(0xC2, 0xDF), Range::new(0x80, 0xBF)]),
                ByteRangeSeq::Three([
                    Range::new(0xE0, 0xE0),
                    Range::new(0xA0, 0xBF),
                    Range::new(0x80, 0xBF)
                ]),
                ByteRangeSeq::Three([
                    Range::new(0xE1, 0xEC),
                    Range::new(0x80, 0xBF),
                    Range::new(0x80, 0xBF)
                ]),
                ByteRangeSeq::Three([
                    Range::new(0xED, 0xED),
                    Range::new(0x80, 0x9F),
                    Range::new(0x80, 0xBF)
                ]),
                ByteRangeSeq::Three([
                    Range::new(0xEE, 0xEF),
                    Range::new(0x80, 0xBF),
                    Range::new(0x80, 0xBF)
                ]),
                ByteRangeSeq::Four([
                    Range::new(0xF0, 0xF0),
                    Range::new(0x90, 0xBF),
                    Range::new(0x80, 0xBF),
                    Range::new(0x80, 0xBF),
                ]),
                ByteRangeSeq::Four([
                    Range::new(0xF1, 0xF3),
                    Range::new(0x80, 0xBF),
                    Range::new(0x80, 0xBF),
                    Range::new(0x80, 0xBF),
                ]),
                ByteRangeSeq::Four([
                    Range::new(0xF4, 0xF4),
                    Range::new(0x80, 0x8F),
                    Range::new(0x80, 0xBF),
                    Range::new(0x80, 0xBF),
                ])
            ]
        );
    }
}
