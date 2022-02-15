#![allow(dead_code)]

mod ast;
mod char;
mod class;
mod compile;
mod curs;
mod dfa;
mod leb128;
mod nfa;
mod parse;
mod prog;
mod range;
mod regex;
mod set;
mod unicode;
mod utf8;

#[cfg(test)]
mod tests {
    #[test]
    fn test() {
        use crate::regex::Regex;

        let regex = Regex::new("\\d(\\d{2})\\d");
        let mut slots = vec![None; 4];
        println!("{:?}", regex.run("xxx1234yyy", &mut slots));
        println!("{:?}", slots);
    }
}
