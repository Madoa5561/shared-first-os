//! Synchronous bounded launch; all acquired resources end before returning.
use crate::{
    launch::{Launch, LaunchError, ReadManifest},
    loader::{Batch, LoadError},
    object::{ObjectStore, ReadBackend, Stats, TASKS},
    scheduler::{MAX_QUANTUM, Snapshot},
    session::{Session, SessionError},
};

pub const MAX_DISPATCHES: usize = 1024;
#[derive(Clone, Copy)]
pub struct RunOptions {
    pub quantum: u32,
    pub dispatch_limit: usize,
    pub manifest: ReadManifest,
    pub fill: u8,
}
#[derive(Debug, PartialEq, Eq)]
pub enum StartError {
    Configuration,
    Load(LoadError),
    Launch(LaunchError),
    Session(SessionError),
}
#[derive(Debug, PartialEq, Eq)]
pub struct Completion {
    pub execution: Snapshot,
    pub dispatches: usize,
    pub limited: bool,
    /// Executing task stops only; the owner is released after this is captured.
    pub stops: [usize; TASKS],
}
#[derive(Debug, PartialEq, Eq)]
pub struct Report {
    pub result: Result<Completion, StartError>,
    pub before: Stats,
    pub after: Stats,
}

/// Trusted kernel policy, not a bytecode syscall. Bounds dispatch count, not wall
/// time; host calls remain synchronous. Ordinary return cleans acquired resources,
/// including rejected setup, but abort/power loss cannot guarantee cleanup.
pub fn execute<B: ReadBackend, const N: usize>(
    store: &mut ObjectStore<B, N>,
    image: &[u8],
    options: RunOptions,
) -> Report {
    let before = store.stats();
    let result = if options.quantum == 0
        || options.quantum > MAX_QUANTUM
        || options.dispatch_limit > MAX_DISPATCHES
    {
        Err(StartError::Configuration)
    } else {
        match Batch::load(image) {
            Err(error) => Err(StartError::Load(error)),
            Ok(mut batch) => {
                let result = execute_batch(store, &batch, options);
                batch.clear();
                result
            }
        }
    };
    Report {
        result,
        before,
        after: store.stats(),
    }
}

fn execute_batch<B: ReadBackend, const N: usize>(
    store: &mut ObjectStore<B, N>,
    batch: &Batch,
    options: RunOptions,
) -> Result<Completion, StartError> {
    let mut launch =
        Launch::new(store, batch, options.manifest, options.fill).map_err(StartError::Launch)?;
    let arguments = launch.arguments();
    let mut session = Session::with_arguments(batch, &mut launch, options.quantum, arguments)
        .map_err(StartError::Session)?;
    let mut dispatches = 0;
    while dispatches < options.dispatch_limit && session.dispatch().is_some() {
        dispatches += 1;
    }
    let limited = session.ready() != 0;
    for task in 0..TASKS {
        session.cancel(task);
    }
    let execution = session.snapshot();
    drop(session);
    Ok(Completion {
        execution,
        dispatches,
        limited,
        stops: launch.stops(),
    })
}
