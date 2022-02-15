#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Range<T> {
    pub start: T,
    pub end: T,
}

impl<T> Range<T> {
    pub const fn new(start: T, end: T) -> Self {
        Self { start, end }
    }

    pub fn contains(&self, value: &T) -> bool
    where
        T: Ord,
    {
        &self.start <= value && value <= &self.end
    }
}
