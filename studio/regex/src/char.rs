pub trait CharExt {
    fn is_ascii_word(self) -> bool;
    fn is_word(self) -> bool;
}

impl CharExt for char {
    fn is_ascii_word(self) -> bool {
        match self {
            '0'..='9' | 'A'..='Z' | '_' | 'a'..='z' => true,
            _ => false,
        }
    }

    fn is_word(self) -> bool {
        use crate::unicode;

        if self.is_ascii() && self.is_ascii_word() {
            return true;
        }
        unicode::WORD
            .binary_search_by(|range| {
                use std::cmp::Ordering;

                if range.end < self {
                    return Ordering::Less;
                }
                if range.start > self {
                    return Ordering::Greater;
                }
                Ordering::Equal
            })
            .is_ok()
    }
}
