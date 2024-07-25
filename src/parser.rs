use std::path::Path;

use syzlang_parser::{
    parser::{Consts, Parsed, Statement},
    token::Token,
};

pub fn parse(path: &Path) -> Parsed {
    let stmts = Statement::from_file(path).unwrap();
    let consts = Consts::new(Vec::new());
    Parsed::new(consts, stmts).unwrap()
}

mod tests {
    use super::*;

    use std::path::Path;

    use env_logger::Builder;
    use log::LevelFilter;

    #[test]
    fn test_parse() {
        Builder::new().filter_level(LevelFilter::Warn).init();
        let path = Path::new("./desc/test.txt");
        let parsed = parse(path);
        println!("{:#?}", parsed);
    }
}
