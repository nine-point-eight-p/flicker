use crate::parse::Args;

use libafl_qemu::Qemu;

pub fn run(args: Args) {
    let mut run_args = args.run_args;
    run_args.insert(0, String::new()); // Add a placeholder for the executable path
    println!("Args: {:?}", run_args);

    let env = Vec::new();
    let qemu = Qemu::init(&run_args, &env).unwrap();

    unsafe {
        match qemu.run() {
            Ok(m) => println!("End with {:?}", m),
            Err(e) => println!("Error when running: {:?}", e),
        }
    }
}
