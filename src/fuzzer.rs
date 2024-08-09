//! A fuzzer using qemu in systemmode for binary-only coverage of kernels
//!
use core::{ptr::addr_of_mut, time::Duration};
use std::{env, path::PathBuf};

use libafl::{
    corpus::{Corpus, InMemoryOnDiskCorpus, OnDiskCorpus},
    events::{launcher::Launcher, EventConfig},
    feedback_or, feedback_or_fast,
    feedbacks::{CrashFeedback, MaxMapFeedback, TimeFeedback, TimeoutFeedback},
    fuzzer::{Fuzzer, StdFuzzer},
    monitors::MultiMonitor,
    mutators::scheduled::StdScheduledMutator,
    observers::{CanTrack, HitcountsMapObserver, TimeObserver, VariableMapObserver},
    schedulers::{IndexesLenTimeMinimizerScheduler, QueueScheduler},
    stages::{CalibrationStage, StdMutationalStage},
    state::{HasCorpus, StdState},
    Error,
};
use libafl_bolts::{
    core_affinity::Cores,
    current_nanos,
    ownedref::OwnedMutSlice,
    rands::StdRand,
    shmem::{ShMemProvider, StdShMemProvider},
    tuples::tuple_list,
};
use libafl_qemu::{
    command::StdCommandManager,
    edges::{edges_map_mut_ptr, QemuEdgeCoverageHelper, EDGES_MAP_SIZE_IN_USE, MAX_EDGES_FOUND},
    emu::Emulator,
    executor::{stateful::StatefulQemuExecutor, QemuExecutorState},
    FastSnapshotManager, QemuHooks, StdEmulatorExitHandler,
};

// use libafl_qemu::QemuSnapshotBuilder; for normal qemu snapshot

use flicker::{
    generator::SyscallGenerator,
    input::SyscallInput,
    mutator::syscall_mutations,
    parser::parse,
    program::{context::Context, metadata::SyscallMetadata},
};

use crate::option::FuzzerOption;

pub fn fuzz(opt: FuzzerOption) {
    let FuzzerOption {
        timeout,
        port: broker_port,
        cores,
        init_corpus,
        gen_corpus,
        crash,
        desc,
        r#const,
        mut run_args,
    } = opt;

    let timeout = Duration::from_secs(timeout);
    let cores = Cores::from_cmdline(&cores).unwrap();
    let init_corpus_dir = PathBuf::from(init_corpus);
    let gen_corpus_dir = PathBuf::from(gen_corpus);
    let crash_dir = PathBuf::from(crash);
    let desc_file = PathBuf::from(desc);
    let const_file = PathBuf::from(r#const);

    // Usually qemu is initialized with `env::args().collect()`,
    // where the first argument is the path of the executable.
    // Since we directly pass arguments into the fuzzer, we add
    // an empty string as a placeholder.
    run_args.insert(0, String::new());

    let metadata = SyscallMetadata::from_parsed(parse(&desc_file, &const_file));

    let mut run_client = |state: Option<_>, mut mgr, _core_id| {
        // Initialize QEMU
        let args: Vec<String> = run_args.clone();
        let env: Vec<(String, String)> = env::vars().collect();

        // let emu_snapshot_manager = QemuSnapshotBuilder::new(true);
        let emu_snapshot_manager = FastSnapshotManager::new(); // Create a snapshot manager (normal or fast for now).
        let emu_exit_handler: StdEmulatorExitHandler<FastSnapshotManager> =
            StdEmulatorExitHandler::new(emu_snapshot_manager); // Create an exit handler: it is the entity taking the decision of what should be done when QEMU returns.

        let cmd_manager = StdCommandManager::new();

        let emu = Emulator::new(&args, &env, emu_exit_handler, cmd_manager).unwrap(); // Create the emulator

        let devices = emu.list_devices();
        println!("Devices = {:?}", devices);

        // The wrapped harness function, calling out to the LLVM-style harness
        let mut harness =
            |input: &SyscallInput, qemu_executor_state: &mut QemuExecutorState<_, _>| unsafe {
                emu.run(input, qemu_executor_state)
                    .unwrap()
                    .try_into()
                    .unwrap()
            };

        // Create an observation channel using the coverage map
        let edges_observer = unsafe {
            HitcountsMapObserver::new(VariableMapObserver::from_mut_slice(
                "edges",
                OwnedMutSlice::from_raw_parts_mut(edges_map_mut_ptr(), EDGES_MAP_SIZE_IN_USE),
                addr_of_mut!(MAX_EDGES_FOUND),
            ))
            .track_indices()
        };

        // Create an observation channel to keep track of the execution time
        let time_observer = TimeObserver::new("time");

        // Feedback to rate the interestingness of an input
        // This one is composed by two Feedbacks in OR
        let mut feedback = feedback_or!(
            // New maximization map feedback linked to the edges observer and the feedback state
            MaxMapFeedback::new(&edges_observer),
            // Time feedback, this one does not need a feedback state
            TimeFeedback::new(&time_observer)
        );

        // A feedback to choose if an input is a solution or not
        let mut objective = feedback_or_fast!(CrashFeedback::new(), TimeoutFeedback::new());

        // If not restarting, create a State from scratch
        let mut state = state.unwrap_or_else(|| {
            StdState::new(
                // RNG
                StdRand::with_seed(current_nanos()),
                // Corpus that will be evolved, we keep it in memory for performance
                InMemoryOnDiskCorpus::new(gen_corpus_dir.clone()).unwrap(),
                // Corpus in which we store solutions (crashes in this example),
                // on disk so the user can get them after stopping the fuzzer
                OnDiskCorpus::new(crash_dir.clone()).unwrap(),
                // States of the feedbacks.
                // The feedbacks can report the data that should persist in the State.
                &mut feedback,
                // Same for objective feedbacks
                &mut objective,
            )
            .unwrap()
        });

        // A minimization+queue policy to get testcasess from the corpus
        let scheduler =
            IndexesLenTimeMinimizerScheduler::new(&edges_observer, QueueScheduler::new());

        // A fuzzer with feedbacks and a corpus scheduler
        let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);

        let mut hooks = QemuHooks::new(
            emu.qemu().clone(),
            tuple_list!(QemuEdgeCoverageHelper::default()),
        );

        // Setup an havoc mutator with a mutational stage
        let mutator = StdScheduledMutator::new(syscall_mutations(metadata.clone()));
        let calibration_feedback = MaxMapFeedback::new(&edges_observer);
        let mut stages = tuple_list!(
            StdMutationalStage::new(mutator),
            CalibrationStage::new(&calibration_feedback)
        );

        // Create a QEMU in-process executor
        let mut executor = StatefulQemuExecutor::new(
            &mut hooks,
            &mut harness,
            tuple_list!(edges_observer, time_observer),
            &mut fuzzer,
            &mut state,
            &mut mgr,
            timeout,
        )
        .expect("Failed to create QemuExecutor");

        // Instead of calling the timeout handler and restart the process, trigger a breakpoint ASAP
        executor.break_on_timeout();

        if state.must_load_initial_inputs() {
            let dirs = [init_corpus_dir.clone(), gen_corpus_dir.clone()];
            if state
                .load_initial_inputs(&mut fuzzer, &mut executor, &mut mgr, &dirs)
                .is_ok()
                && state.corpus().count() > 0
            {
                println!("We imported {} inputs from disk.", state.corpus().count());
            } else {
                println!("Failed to import initial inputs, try to generate");
                let context = Context::new(metadata.clone());
                let mut generator = SyscallGenerator::new(64, context);
                state
                    .generate_initial_inputs(
                        &mut fuzzer,
                        &mut executor,
                        &mut generator,
                        &mut mgr,
                        4,
                    )
                    .expect("Failed to generate initial corpus");
                println!("We generated {} inputs.", state.corpus().count());
            }
        }

        fuzzer
            .fuzz_loop(&mut stages, &mut executor, &mut state, &mut mgr)
            .unwrap();
        Ok(())
    };

    // The shared memory allocator
    let shmem_provider = StdShMemProvider::new().expect("Failed to init shared memory");

    // The stats reporter for the broker
    let monitor = MultiMonitor::new(|s| println!("{s}"));

    // let monitor = SimpleMonitor::new(|s| println!("{s}"));
    // let mut mgr = SimpleEventManager::new(monitor);
    // run_client(None, mgr, 0);

    // Build and run a Launcher
    match Launcher::builder()
        .shmem_provider(shmem_provider)
        .broker_port(broker_port)
        .configuration(EventConfig::from_build_id())
        .monitor(monitor)
        .run_client(&mut run_client)
        .cores(&cores)
        // .stdout_file(Some("/dev/null"))
        .build()
        .launch()
    {
        Ok(()) => (),
        Err(Error::ShuttingDown) => println!("Fuzzing stopped by user. Good bye."),
        Err(err) => panic!("Failed to run launcher: {err:?}"),
    }
}
