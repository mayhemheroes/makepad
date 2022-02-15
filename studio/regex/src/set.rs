#[derive(Clone, Debug)]
pub struct Set {
    dense: Vec<usize>,
    sparse: Box<[usize]>,
}

impl Set {
    pub fn new(capacity: usize) -> Self {
        Self {
            dense: Vec::with_capacity(capacity),
            sparse: vec![0; capacity].into_boxed_slice(),
        }
    }

    pub fn as_slice(&self) -> &[usize] {
        self.dense.as_slice()
    }

    pub fn contains(&self, value: usize) -> bool {
        self.dense.get(self.sparse[value]) == Some(&value)
    }

    pub fn insert(&mut self, value: usize) -> bool {
        if self.contains(value) {
            return false;
        }
        let index = self.dense.len();
        self.dense.push(value);
        self.sparse[value] = index;
        true
    }

    pub fn clear(&mut self) {
        self.dense.clear()
    }
}
