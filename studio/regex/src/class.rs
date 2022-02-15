use {
    crate::range::Range,
    std::{collections::BTreeMap, slice},
};

#[derive(Clone, Debug)]
pub struct Class {
    bounds: Vec<u32>,
}

impl Class {
    pub fn new() -> Self {
        Self { bounds: Vec::new() }
    }

    pub fn from_ranges(ranges: &[Range<char>], negated: bool) -> Self {
        let mut bounds = Vec::new();
        let mut push_range = |range: Range<u32>| {
            bounds.push(range.start);
            bounds.push(range.end + 1);
        };
        if negated {
            negate_ranges(ranges, |range| {
                split_range(range, |range| {
                    push_range(range);
                });
            });
        } else {
            for &range in ranges {
                split_range(Range::new(range.start as u32, range.end as u32), |range| {
                    push_range(range);
                });
            }
        }
        Self { bounds }
    }

    pub fn iter(&self) -> Iter<'_> {
        Iter {
            iter: self.bounds.iter(),
        }
    }
}

impl<'a> IntoIterator for &'a Class {
    type Item = Range<char>;
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct Iter<'a> {
    iter: slice::Iter<'a, u32>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = Range<char>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(Range {
            start: char::from_u32(*self.iter.next()?).unwrap(),
            end: char::from_u32(*self.iter.next()? - 1).unwrap(),
        })
    }
}

#[derive(Debug)]
pub struct Builder {
    deltas: BTreeMap<u32, i32>,
}

impl Builder {
    pub fn new() -> Self {
        Self {
            deltas: BTreeMap::new(),
        }
    }

    pub fn insert_ranges(&mut self, ranges: &[Range<char>], negated: bool) {
        if negated {
            negate_ranges(ranges, |range| {
                split_range(range, |range| self.insert_range(range))
            })
        } else {
            for &range in ranges {
                split_range(Range::new(range.start as u32, range.end as u32), |range| {
                    self.insert_range(range)
                });
            }
        }
    }

    pub fn build(&mut self, negated: bool) -> Class {
        let mut bounds = Vec::new();
        if negated {
            if !self.deltas.contains_key(&0) {
                bounds.push(0);
            }
            let mut count = 0;
            for (&bound, &delta) in self.deltas.range(1..0xD800) {
                let next_count = count + delta;
                if (count != 0) != (next_count != 0) {
                    bounds.push(bound);
                }
                count = next_count;
            }
            if !self.deltas.contains_key(&0xD800) {
                bounds.push(0xD800);
            }
            if !self.deltas.contains_key(&0xE000) {
                bounds.push(0xE000);
            }
            let mut count = 0;
            for (&bound, &delta) in self.deltas.range(0xE001..0x110000) {
                let next_count = count + delta;
                if (count != 0) != (next_count != 0) {
                    bounds.push(bound);
                }
                count = next_count;
            }
            if !self.deltas.contains_key(&0x110000) {
                bounds.push(0x110000);
            }
        } else {
            let mut count = 0;
            for (&bound, &delta) in &self.deltas {
                let next_count = count + delta;
                if (count != 0) != (next_count != 0) {
                    bounds.push(bound);
                }
                count = next_count;
            }
        }
        self.deltas.clear();
        Class { bounds }
    }

    fn insert_range(&mut self, range: Range<u32>) {
        use std::collections::btree_map::Entry;

        match self.deltas.entry(range.start as u32) {
            Entry::Occupied(mut entry) => {
                if *entry.get() == -1 {
                    entry.remove();
                } else {
                    *entry.get_mut() += 1;
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(1);
            }
        }
        match self.deltas.entry(range.end as u32 + 1) {
            Entry::Occupied(mut entry) => {
                if *entry.get() == 1 {
                    entry.remove();
                } else {
                    *entry.get_mut() -= 1;
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(-1);
            }
        }
    }
}

fn negate_ranges<F: FnMut(Range<u32>)>(ranges: &[Range<char>], mut f: F) {
    if ranges.is_empty() {
        return;
    }
    let first_range_start = ranges.first().unwrap().start as u32;
    if first_range_start > 0 {
        f(Range::new(0, first_range_start - 1));
    }
    for window in ranges.windows(2) {
        let previous_range_end = window[0].end as u32;
        let next_range_start = window[1].start as u32;
        assert!(previous_range_end + 1 < next_range_start);
        f(Range::new(previous_range_end + 1, next_range_start - 1));
    }
    let last_range_end = ranges.last().unwrap().end as u32;
    if last_range_end < 0x10FFFF {
        f(Range::new(last_range_end + 1, 0x10FFFF));
    }
}

fn split_range<F: FnMut(Range<u32>)>(range: Range<u32>, mut f: F) {
    if range.start <= 0xD7FF && range.end >= 0xE000 {
        f(Range::new(range.start, 0xD7FF));
        f(Range::new(0xE000, range.end));
        return;
    }
    f(range)
}
