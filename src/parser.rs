use std::path::Path;

use syzlang_parser::{
    parser::{Consts, Parsed, Statement},
    token::Token,
};

pub fn parse(desc_path: &Path, const_path: &Path) -> Parsed {
    let stmts = Statement::from_file(desc_path).unwrap();
    let mut consts = Consts::new(Vec::new());
    consts.create_from_file(const_path).unwrap();
    Parsed::new(consts, stmts).unwrap()
}
