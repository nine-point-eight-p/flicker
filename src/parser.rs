use std::path::Path;

use syzlang_parser::parser::{Consts, Parsed, Statement};

pub fn parse(desc_path: &Path, const_path: &Path) -> Parsed {
    println!("Parsing files: {:?}, {:?}", desc_path, const_path);

    let builtin = Statement::from_file(Path::new("desc/builtin.txt")).unwrap();
    let desc = Statement::from_file(desc_path).unwrap();
    let stmts = [builtin, desc].concat();

    let mut consts = Consts::new(Vec::new());
    consts.create_from_file(const_path).unwrap();

    let mut parsed = Parsed::new(consts, stmts).unwrap();
    parsed.postprocess().unwrap();
    parsed
}
