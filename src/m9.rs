//! External module -> owned validated programs -> M8 scheduler, no ambient rights.
use super::*;
use shared_first_os::{
    loader::Batch,
    scheduler::{Scheduler, TaskHost},
    session::Session,
};

struct World<'a> {
    store: &'a mut Bench,
    stops: usize,
}
impl TaskHost for World<'_> {
    fn sum(&mut self, task: usize, handle: u64, offset: u64, length: u64) -> Result<u64, Fault> {
        BoundHost {
            store: self.store,
            task,
        }
        .sum(handle, offset, length)
    }
    fn stop(&mut self, task: usize) {
        self.store.release_task(task).unwrap();
        self.stops += 1;
    }
}
fn fingerprint(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}
pub(super) fn run(store: &mut Bench) {
    let baseline = accounting();
    assert_eq!(store.stats().capabilities + store.stats().payload_frames, 0);
    let module = match crate::boot::bytecode_module() {
        Ok(module) => module,
        Err(error) => {
            serial::print(format_args!(
                "M9REJECT transport={} admitted=0 cleanup=1\n",
                error
            ));
            return;
        }
    };
    serial::print(format_args!(
        "M9MODULE bytes={} physical={:#x} retained_pages={} fingerprint={}\n",
        module.bytes.len(),
        module.physical,
        module.retained_pages,
        fingerprint(module.bytes)
    ));
    let mut batch = match Batch::load(module.bytes) {
        Ok(batch) => batch,
        Err(error) => {
            assert_eq!(accounting(), baseline);
            serial::print(format_args!(
                "M9REJECT format={:?} admitted=0 cleanup=1\n",
                error
            ));
            return;
        }
    };
    serial::print(format_args!(
        "M9LOAD tasks={} code_bytes={} fuel={} batch_bytes={} scheduler_bytes={}\n",
        batch.count(),
        batch.code_bytes(),
        batch.fuel(),
        size_of::<Batch>(),
        size_of::<Scheduler>()
    ));
    let turns;
    let stops;
    {
        let mut host = World { store, stops: 0 };
        let mut session = Session::new(&batch, &mut host, 2).unwrap();
        let mut dispatched = 0;
        while let Some(event) = session.dispatch() {
            dispatched += 1;
            // Bound diagnostics as well as execution, even for a hostile package.
            // At most 64 dispatches printed; all later work is still fuel bounded.
            if dispatched <= 64 {
                serial::print(format_args!(
                    "M9TASK task={} result={:?} before={} after={}\n",
                    event.task, event.result, event.fuel_before, event.fuel_after
                ));
            }
        }
        drop(session);
        assert_eq!(host.stops, batch.count());
        turns = dispatched;
        stops = host.stops;
    }
    batch.clear();
    assert_eq!(accounting(), baseline);
    assert_eq!(store.stats().capabilities + store.stats().payload_frames, 0);
    serial::print(format_args!(
        "M9CLEAN turns={} stops={} code_bytes={} tasks={} before={} after={} tables={} mappings={} capabilities=0 payload=0\nM9 PASS\n",
        turns,
        stops,
        batch.code_bytes(),
        batch.count(),
        baseline.0,
        accounting().0,
        accounting().1,
        accounting().2
    ));
}
