//! Verify scoped cancellation against real object backing and reference counts.
use super::*;
use shared_first_os::{
    loader::{Batch, MAGIC},
    sandbox::Slice,
    scheduler::TaskHost,
    session::Session,
};
struct World<'a> {
    store: &'a mut Bench,
    stops: [usize; 4],
    calls: usize,
}
impl TaskHost for World<'_> {
    fn sum(&mut self, task: usize, handle: u64, offset: u64, length: u64) -> Result<u64, Fault> {
        self.calls += 1;
        BoundHost {
            store: self.store,
            task,
        }
        .sum(handle, offset, length)
    }
    fn stop(&mut self, task: usize) {
        self.store.release_task(task).unwrap();
        self.stops[task] += 1;
    }
}
fn input(fault: bool) -> [u8; 112] {
    let mut bytes = [0; 112];
    bytes[..8].copy_from_slice(&MAGIC);
    bytes[8] = 1;
    bytes[10] = 3;
    for index in 0..3 {
        let offset = 16 + index * 32;
        bytes[offset] = 16;
        bytes[offset + 4] = 4;
        let instruction = if fault && index == 0 {
            ins(2, 0, 0, 0, 1)
        } else {
            ins(0, 0, 0, 0, 0)
        };
        bytes[offset + 16..offset + 32].copy_from_slice(&instruction);
    }
    bytes
}
pub(super) fn run(store: &mut Bench) {
    for case in 0..4 {
        let baseline = accounting();
        runtime(|state| state.parts().0.frames.reset_peak());
        let owner = store.create_filled(0, 4096, 0x5a).unwrap();
        store.grant(0, owner, 1, Rights::READ).unwrap();
        store.grant(0, owner, 2, Rights::READ).unwrap();
        let mut batch = Batch::load(&input(case == 3)).unwrap();
        let mut host = World {
            store,
            stops: [0; 4],
            calls: 0,
        };
        let turns = usize::from(case != 0);
        {
            let mut session = Session::new(&batch, &mut host, 2).unwrap();
            if case == 2 {
                assert!(session.cancel(1));
                assert!(!session.cancel(1));
            }
            if case != 0 {
                let event = session.dispatch().unwrap();
                assert_eq!(event.task, 0);
                assert_eq!(
                    event.result,
                    if case == 3 {
                        Err(Fault::Authority)
                    } else {
                        Ok(Slice::Event(Outcome::Halt(0)))
                    }
                );
            }
            assert_eq!(
                session.ready(),
                if case == 0 {
                    3
                } else if case == 2 {
                    1
                } else {
                    2
                }
            );
            // Scope exit deliberately leaves live tasks: no explicit shutdown.
        }
        assert_eq!(host.stops, [1, 1, 1, 0]);
        assert_eq!(host.calls, usize::from(case == 3));
        assert_eq!(
            host.store.stats().payload_frames + host.store.stats().capabilities,
            0
        );
        batch.clear();
        assert_eq!(batch.count() + batch.code_bytes(), 0);
        assert_eq!(accounting(), baseline);
        let peak = runtime(|state| state.parts().0.frames.peak());
        assert_eq!(peak, baseline.0 + 1);
        serial::print(format_args!(
            "M10CASE case={} turns={} stops=3 calls={} before={} after={} peak={} tables={} mappings={} payload=0 capabilities=0 code_bytes=0\n",
            case,
            turns,
            host.calls,
            baseline.0,
            accounting().0,
            peak,
            baseline.1,
            baseline.2
        ));
    }
    serial::print(format_args!(
        "M10CHECK cases=4 session_bytes={} host_bound=1 drop_cleanup=1 once=1 code_lifetime=1\nM10 PASS\n",
        size_of::<Session<'_, '_, World<'_>>>()
    ));
}
