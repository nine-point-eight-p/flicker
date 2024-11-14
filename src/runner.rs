//! LibAFL QEMU based runner to reproduce a crash.

use std::time::Duration;

#[cfg(feature = "bytes")]
use libafl::inputs::BytesInput;
use libafl::{
    corpus::NopCorpus, events::SimpleEventManager, inputs::Input, monitors::SimpleMonitor,
    schedulers::QueueScheduler, state::StdState, StdFuzzer,
};
use libafl_bolts::{rands::StdRand, tuples::tuple_list};
use libafl_qemu::{executor::QemuExecutor, Emulator};

use crate::cli::ReproduceOption;

#[cfg(not(feature = "bytes"))]
use flicker::input::SyscallInput;

pub fn reproduce(opt: ReproduceOption) {
    let ReproduceOption {
        testcase,
        timeout,
        mut args,
    } = opt;

    let timeout = Duration::from_secs(timeout);

    // Usually qemu is initialized with `env::args().collect()`,
    // where the first argument is the path of the executable.
    // Since we directly pass arguments into the fuzzer, we add
    // an empty string as a placeholder.
    args.insert(0, String::new());

    // Initialize QEMU
    #[cfg(not(feature = "bytes"))]
    type Input = SyscallInput;
    #[cfg(feature = "bytes")]
    type Input = BytesInput;

    let emulator = Emulator::builder()
        .qemu_cli(args)
        .build()
        .expect("Failed to initialize QEMU");

    // The wrapped harness function, calling out to the LLVM-style harness
    let mut harness = |emu: &mut Emulator<_, _, _, _, _>, state: &mut _, input: &Input| unsafe {
        emu.run(state, input).unwrap().try_into().unwrap()
    };

    // Load input and make it into a corpus
    let input = Input::from_file(&testcase).unwrap();
    println!("Loaded input: {:?}", input);
    // let testcase = Testcase::new(input);
    // let mut corpus = InMemoryCorpus::new();
    // corpus.add(testcase).unwrap();

    // Empty feedback and objective
    let mut feedback = ();
    let mut objective = ();

    // Create state with single-input corpus and empty solution
    let mut state = StdState::new(
        StdRand::new(),
        NopCorpus::new(),
        NopCorpus::new(),
        &mut feedback,
        &mut objective,
    )
    .unwrap();

    // A simple queue scheduler
    let scheduler = QueueScheduler::new();

    // A fuzzer with feedbacks and a corpus scheduler
    let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);

    // Create monitor and event manager
    let monitor = SimpleMonitor::with_user_monitor(|s| {
        println!("{s}");
    });
    let mut event_mgr = SimpleEventManager::new(monitor);

    // Create a QEMU in-process executor
    let mut executor = QemuExecutor::new(
        emulator,
        &mut harness,
        tuple_list!(),
        &mut fuzzer,
        &mut state,
        &mut event_mgr,
        timeout,
    )
    .expect("Failed to create QemuExecutor");
    executor.break_on_timeout();

    match fuzzer.execute_input(&mut state, &mut executor, &mut event_mgr, &input) {
        Ok(kind) => println!("Execution succeeded: {:?}", kind),
        Err(e) => println!("Execution failed: {}", e),
    }
}
