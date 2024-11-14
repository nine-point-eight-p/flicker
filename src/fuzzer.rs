//! A binary-only kernel fuzzer using LibAFL QEMU in systemmode

use std::time::Duration;

use libafl::{
    corpus::{Corpus, InMemoryOnDiskCorpus, OnDiskCorpus},
    events::{EventConfig, Launcher},
    feedback_or, feedback_or_fast,
    feedbacks::{CrashFeedback, MaxMapFeedback, TimeFeedback, TimeoutFeedback},
    fuzzer::{Fuzzer, StdFuzzer},
    monitors::MultiMonitor,
    mutators::StdScheduledMutator,
    observers::{CanTrack, HitcountsMapObserver, TimeObserver, VariableMapObserver},
    schedulers::{IndexesLenTimeMinimizerScheduler, QueueScheduler},
    stages::{CalibrationStage, StdMutationalStage},
    state::{HasCorpus, HasMaxSize, StdState},
    Error,
};
#[cfg(feature = "bytes")]
use libafl::{generators::RandBytesGenerator, inputs::BytesInput, mutators::havoc_mutations};
use libafl_bolts::{
    core_affinity::Cores,
    current_nanos,
    ownedref::OwnedMutSlice,
    rands::StdRand,
    shmem::{ShMemProvider, StdShMemProvider},
    tuples::tuple_list,
};
use libafl_qemu::{executor::QemuExecutor, modules::StdEdgeCoverageClassicModule, Emulator};
use libafl_targets::{edges_map_mut_ptr, EDGES_MAP_DEFAULT_SIZE, MAX_EDGES_FOUND};

#[cfg(not(feature = "bytes"))]
use flicker::{
    generator::SyscallGenerator,
    input::SyscallInput,
    mutator::syscall_mutations,
    parser::parse,
    program::{context::Context, metadata::SyscallMetadata},
};

use crate::cli::FuzzOption;

// /// Metadata for testcases for reproduction.
// #[derive(Debug, Clone, Serialize, Deserialize, SerdeAny)]
// pub struct TestcaseMetadata {
//     /// Path to the description file
//     desc: String,
//     /// Path to the constants file
//     r#const: String,
//     /// Arguments passed to Qemu
//     run_args: Vec<String>,
// }

pub fn fuzz(opt: FuzzOption) {
    let FuzzOption {
        timeout,
        port: broker_port,
        cores,
        init_corpus,
        gen_corpus,
        crash,
        #[cfg(not(feature = "bytes"))]
        desc,
        #[cfg(not(feature = "bytes"))]
        r#const,
        #[cfg(not(feature = "bytes"))]
        max_calls,
        #[cfg(feature = "bytes")]
        max_size,
        mut args,
    } = opt;

    let timeout = Duration::from_secs(timeout);
    let cores = Cores::from_cmdline(&cores).unwrap();
    // TODO: Add cli options to testcases as metadata
    // let testcase_metadata = TestcaseMetadata {
    //     desc,
    //     r#const,
    //     run_args,
    // };

    // Usually qemu is initialized with `env::args().collect()`,
    // where the first argument is the path of the executable.
    // Since we directly pass arguments into the fuzzer, we add
    // an empty string as a placeholder.
    args.insert(0, String::new());

    #[cfg(not(feature = "bytes"))]
    let syscall_metadata = SyscallMetadata::from_parsed(parse(&desc, &r#const));

    let mut run_client = |state: Option<_>, mut mgr, _core_id| {
        // Choose modules
        let modules = tuple_list!(StdEdgeCoverageClassicModule::builder()
            .build()
            .expect("Failed to create coverage module"));

        // Initialize QEMU
        let emulator = Emulator::builder()
            .qemu_cli(args.clone())
            .modules(modules)
            .build()
            .expect("Failed to initialize QEMU");

        let devices = emulator.list_devices();
        println!("Devices = {:?}", devices);

        // The wrapped harness function, calling out to the LLVM-style harness
        #[cfg(not(feature = "bytes"))]
        type Input = SyscallInput;
        #[cfg(feature = "bytes")]
        type Input = BytesInput;

        let mut harness = |emu: &mut Emulator<_, _, _, _, _>, state: &mut _, input: &Input| unsafe {
            emu.run(state, input).unwrap().try_into().unwrap()
        };

        // Create an observation channel using the coverage map
        let edges_observer = unsafe {
            HitcountsMapObserver::new(VariableMapObserver::from_mut_slice(
                "edges",
                OwnedMutSlice::from_raw_parts_mut(edges_map_mut_ptr(), EDGES_MAP_DEFAULT_SIZE),
                &raw mut MAX_EDGES_FOUND,
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
            let mut new_state = StdState::new(
                // RNG
                StdRand::with_seed(current_nanos()),
                // Corpus that will be evolved, we keep it in memory for performance
                InMemoryOnDiskCorpus::new(gen_corpus.clone()).unwrap(),
                // Corpus in which we store solutions (crashes in this example),
                // on disk so the user can get them after stopping the fuzzer
                OnDiskCorpus::new(crash.clone()).unwrap(),
                // States of the feedbacks.
                // The feedbacks can report the data that should persist in the State.
                &mut feedback,
                // Same for objective feedbacks
                &mut objective,
            )
            .unwrap();
            #[cfg(not(feature = "bytes"))]
            new_state.set_max_size(max_calls);
            #[cfg(feature = "bytes")]
            new_state.set_max_size(max_size);
            new_state
        });

        // A minimization+queue policy to get testcasess from the corpus
        let scheduler =
            IndexesLenTimeMinimizerScheduler::new(&edges_observer, QueueScheduler::new());

        // A fuzzer with feedbacks and a corpus scheduler
        let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);

        // Setup a syscall mutator with a mutational stage
        #[cfg(not(feature = "bytes"))]
        let mutator = StdScheduledMutator::new(syscall_mutations(syscall_metadata.clone()));
        #[cfg(feature = "bytes")]
        let mutator = StdScheduledMutator::new(havoc_mutations());
        let calibration_feedback = MaxMapFeedback::new(&edges_observer);
        let mut stages = tuple_list!(
            StdMutationalStage::new(mutator),
            CalibrationStage::new(&calibration_feedback)
        );

        // Create a QEMU in-process executor
        let mut executor = QemuExecutor::new(
            emulator,
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
            let dirs = [init_corpus.clone(), gen_corpus.clone()];
            if state
                .load_initial_inputs(&mut fuzzer, &mut executor, &mut mgr, &dirs)
                .is_ok()
                && state.corpus().count() > 0
            {
                println!("We imported {} inputs from disk.", state.corpus().count());
            } else {
                println!("Failed to import initial inputs, try to generate");
                #[cfg(not(feature = "bytes"))]
                let context = Context::new(syscall_metadata.clone());
                #[cfg(not(feature = "bytes"))]
                let mut generator = SyscallGenerator::new(max_calls, context);
                #[cfg(feature = "bytes")]
                let mut generator = RandBytesGenerator::new(max_size.try_into().unwrap());
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
