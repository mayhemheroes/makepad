pub trait Cursor {
    type Pos: Copy;

    fn pos(&self) -> Self::Pos;
    fn next_byte(&self) -> Option<u8>;
    fn prev_byte(&self) -> Option<u8>;
    fn next_char(&self) -> Option<char>;
    fn prev_char(&self) -> Option<char>;
    fn set_pos(&mut self, pos: Self::Pos);
    fn move_forward(&mut self, n: usize);
    fn move_backward(&mut self, n: usize);

    fn by_ref(&mut self) -> &mut Self
    where
        Self: Sized,
    {
        self
    }

    fn text_start(&self) -> bool {
        self.prev_byte().is_none()
    }

    fn text_end(&self) -> bool {
        self.next_byte().is_none()
    }

    fn word_boundary(&self) -> bool {
        use crate::char::CharExt;

        self.prev_char().map(|c| c.is_word()) != self.next_char().map(|c| c.is_word())
    }

    fn rev(self) -> Rev<Self>
    where
        Self: Sized,
    {
        Rev { curs: self }
    }
}

impl<'a, C: Cursor> Cursor for &'a mut C {
    type Pos = C::Pos;

    fn pos(&self) -> Self::Pos {
        (**self).pos()
    }

    fn next_byte(&self) -> Option<u8> {
        (**self).next_byte()
    }

    fn prev_byte(&self) -> Option<u8> {
        (**self).prev_byte()
    }

    fn next_char(&self) -> Option<char> {
        (**self).next_char()
    }

    fn prev_char(&self) -> Option<char> {
        (**self).prev_char()
    }

    fn set_pos(&mut self, pos: Self::Pos) {
        (**self).set_pos(pos)
    }

    fn move_forward(&mut self, n: usize) {
        (**self).move_forward(n)
    }

    fn move_backward(&mut self, n: usize) {
        (**self).move_backward(n)
    }
}

pub struct Rev<C> {
    curs: C,
}

impl<C: Cursor> Cursor for Rev<C> {
    type Pos = C::Pos;

    fn pos(&self) -> Self::Pos {
        self.curs.pos()
    }

    fn next_byte(&self) -> Option<u8> {
        self.curs.prev_byte()
    }

    fn prev_byte(&self) -> Option<u8> {
        self.curs.next_byte()
    }

    fn next_char(&self) -> Option<char> {
        self.curs.prev_char()
    }

    fn prev_char(&self) -> Option<char> {
        self.curs.next_char()
    }

    fn set_pos(&mut self, pos: Self::Pos) {
        self.curs.set_pos(pos)
    }

    fn move_forward(&mut self, n: usize) {
        self.curs.move_backward(n);
    }

    fn move_backward(&mut self, n: usize) {
        self.curs.move_forward(n);
    }
}

#[derive(Debug)]
pub struct StrCurs<'a> {
    str: &'a str,
    pos: usize,
}

impl<'a> Cursor for StrCurs<'a> {
    type Pos = usize;

    fn pos(&self) -> Self::Pos {
        self.pos
    }

    fn next_byte(&self) -> Option<u8> {
        self.str.as_bytes().get(self.pos).cloned()
    }

    fn prev_byte(&self) -> Option<u8> {
        self.pos.checked_sub(1).map(|pos| self.str.as_bytes()[pos])
    }

    fn next_char(&self) -> Option<char> {
        self.str.split_at(self.pos).1.chars().next()
    }

    fn prev_char(&self) -> Option<char> {
        self.str.split_at(self.pos).0.chars().next_back()
    }

    fn set_pos(&mut self, pos: Self::Pos) {
        self.pos = pos;
    }

    fn move_forward(&mut self, n: usize) {
        assert!(self.pos + n <= self.str.len());
        self.pos += n;
    }

    fn move_backward(&mut self, n: usize) {
        assert!(self.pos >= n);
        self.pos -= n;
    }
}

pub trait IntoCursor {
    type Pos: Copy;
    type IntoCurs: Cursor<Pos = Self::Pos>;

    fn into_curs(self) -> Self::IntoCurs;
}

impl<C: Cursor> IntoCursor for C {
    type Pos = C::Pos;
    type IntoCurs = C;

    fn into_curs(self) -> Self::IntoCurs {
        self
    }
}

impl<'a> IntoCursor for &'a str {
    type Pos = usize;
    type IntoCurs = StrCurs<'a>;

    fn into_curs(self) -> Self::IntoCurs {
        StrCurs { str: self, pos: 0 }
    }
}
