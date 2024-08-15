//! LibAFL QEMU based runner to reproduce a crash.

use std::env;
use std::path::PathBuf;
use std::time::Duration;

use libafl::{
    corpus::NopCorpus, events::SimpleEventManager, inputs::Input, monitors::SimpleMonitor,
    prelude::QueueScheduler, state::StdState, StdFuzzer,
};
use libafl_bolts::{rands::StdRand, tuples::tuple_list};
use libafl_qemu::{
    command::StdCommandManager,
    executor::{stateful::StatefulQemuExecutor, QemuExecutorState},
    Emulator, FastSnapshotManager, QemuHooks, StdEmulatorExitHandler,
};

use crate::cli::ReproduceOption;
use flicker::input::SyscallInput;

pub fn reproduce(opt: ReproduceOption) {
    let ReproduceOption {
        testcase,
        timeout,
        mut run_args,
    } = opt;

    let timeout = Duration::from_secs(timeout);
    let testcase = PathBuf::from(testcase);

    // Usually qemu is initialized with `env::args().collect()`,
    // where the first argument is the path of the executable.
    // Since we directly pass arguments into the fuzzer, we add
    // an empty string as a placeholder.
    run_args.insert(0, String::new());

    // Create monitor and event manager
    let monitor = SimpleMonitor::with_user_monitor(|s| {
        println!("{s}");
    });
    let mut event_mgr = SimpleEventManager::new(monitor);

    // Load input and make it into a corpus
    let input = SyscallInput::from_file(&testcase).unwrap();
    println!("Loaded input: {:?}", input);
    // let testcase = Testcase::new(input);
    // let mut corpus = InMemoryCorpus::new();
    // corpus.add(testcase).unwrap();

    // Initialize QEMU
    let env: Vec<(String, String)> = env::vars().collect();
    let emu_snapshot_manager = FastSnapshotManager::new();
    let emu_exit_handler: StdEmulatorExitHandler<FastSnapshotManager> =
        StdEmulatorExitHandler::new(emu_snapshot_manager);
    let cmd_manager = StdCommandManager::new();
    let emu = Emulator::new(&run_args, &env, emu_exit_handler, cmd_manager).unwrap();

    // The wrapped harness function, calling out to the LLVM-style harness
    let mut harness = |input: &SyscallInput, qemu_executor_state: &mut QemuExecutorState<_, _>| unsafe {
        emu.run(input, qemu_executor_state)
            .unwrap()
            .try_into()
            .unwrap()
    };

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
    let mut fuzzer = StdFuzzer::<_, _, _, ()>::new(scheduler, feedback, objective);

    // Empty hooks
    let mut hooks = QemuHooks::new(emu.qemu().clone(), tuple_list!());

    // Create a QEMU in-process executor
    let mut executor = StatefulQemuExecutor::new(
        &mut hooks,
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
