pub fn read_isize(bytes: &mut &[u8]) -> Option<isize> {
    let un = read_usize(bytes)?;
    let mut n = (un >> 1) as isize;
    if un & 1 != 0 {
        n = !n;
    }
    Some(n)
}

pub fn read_usize(bytes: &mut &[u8]) -> Option<usize> {
    let mut n = 0;
    let mut shift = 0;
    while !bytes.is_empty() {
        let b = bytes[0];
        *bytes = &bytes[1..];
        n |= ((b & 0x7F) as usize) << shift;
        if b < 0x80 {
            return Some(n);
        }
        shift += 7;
    }
    None
}

pub fn write_isize(bytes: &mut Vec<u8>, n: isize) {
    let mut un = (n as usize) << 1;
    if n < 0 {
        un = !un;
    }
    write_usize(bytes, un)
}

pub fn write_usize(bytes: &mut Vec<u8>, n: usize) {
    let mut n = n;
    while n >= 0x80 {
        bytes.push((n as u8) | 0x80);
        n >>= 7;
    }
    bytes.push(n as u8);
}
