//! Scoped revocation, partial cancellation and generation-safe restart.
use super::*;
use shared_first_os::{
    launch::{DEMO, Launch},
    loader::Batch,
    scheduler::{Dispatch, TaskHost},
    session::{RevocableHost, Session},
};
struct World<'a> {
    launch: Launch<'a, Hardware, 64>,
    case: usize,
    reads: usize,
}
impl TaskHost for World<'_> {
    fn sum(&mut self, task: usize, handle: u64, offset: u64, length: u64) -> Result<u64, Fault> {
        reset_trace();
        let result = self.launch.sum(task, handle, offset, length);
        let observed = trace();
        self.reads += 1;
        if self.reads <= 64 {
            serial::print(format_args!(
                "M12READ case={} task={} allowed={} length={} value={} frame={:#x} pages={}\n",
                self.case,
                task,
                usize::from(result.is_ok()),
                length,
                result.unwrap_or(0),
                if observed.count == 0 {
                    0
                } else {
                    observed.frames[0]
                },
                observed.count
            ));
        }
        result
    }
    fn stop(&mut self, task: usize) {
        self.launch.stop(task);
    }
}
impl RevocableHost for World<'_> {
    fn revoke(&mut self) -> bool {
        let before = accounting().0;
        let changed = self.launch.revoke();
        let stats = self.launch.stats();
        assert_eq!(stats.payload_frames + stats.capabilities, 0);
        serial::print(format_args!(
            "M12REVOKE case={} changed={} before={} after={} payload={} capabilities={}\n",
            self.case,
            usize::from(changed),
            before,
            accounting().0,
            stats.payload_frames,
            stats.capabilities
        ));
        changed
    }
}
fn record(case: usize, event: Dispatch) {
    serial::print(format_args!(
        "M12TASK case={} task={} result={:?} before={} after={}\n",
        case, event.task, event.result, event.fuel_before, event.fuel_after
    ));
}
pub(super) fn run(store: &mut Bench) {
    let baseline = accounting();
    assert_eq!(store.stats().objects + store.stats().capabilities, 0);
    let module = match crate::boot::bytecode_module(2) {
        Ok(module) => module,
        Err(error) => {
            serial::print(format_args!(
                "M12REJECT transport={} admitted=0 grants=0\n",
                error
            ));
            return;
        }
    };
    let fingerprint = module
        .bytes
        .iter()
        .fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    serial::print(format_args!(
        "M12MODULE bytes={} physical={:#x} retained_pages={} fingerprint={}\n",
        module.bytes.len(),
        module.physical,
        module.retained_pages,
        fingerprint
    ));
    let mut batch = match Batch::load(module.bytes) {
        Ok(batch) => batch,
        Err(error) => {
            assert_eq!(accounting(), baseline);
            serial::print(format_args!(
                "M12REJECT format={:?} admitted=0 grants=0\n",
                error
            ));
            return;
        }
    };
    let mut old = 0;
    for case in 0..3 {
        runtime(|state| state.parts().0.frames.reset_peak());
        let turns;
        {
            let launch = match Launch::new(store, &batch, DEMO, 7) {
                Ok(launch) => launch,
                Err(error) => {
                    assert_eq!(accounting(), baseline);
                    serial::print(format_args!(
                        "M12REJECT manifest={:?} admitted=0 grants=0\n",
                        error
                    ));
                    return;
                }
            };
            let mut arguments = launch.arguments();
            if case == 0 {
                old = arguments[0];
            }
            if case == 2 {
                assert_ne!(old, arguments[0]);
                serial::print(format_args!(
                    "M12RESTART old={} new={} foreign={}\n",
                    old, arguments[0], arguments[1]
                ));
                arguments[0] = old;
                arguments[2] = arguments[1];
            }
            let mut host = World {
                launch,
                case,
                reads: 0,
            };
            let mut session = Session::with_arguments(&batch, &mut host, 2, arguments).unwrap();
            let mut count = 0;
            if case < 2 {
                for _ in 0..2 {
                    record(case, session.dispatch().unwrap());
                    count += 1;
                }
                let cancelled = if case == 1 {
                    assert!(session.cancel(0));
                    assert!(!session.cancel(0));
                    1
                } else {
                    0
                };
                let ready = session.ready();
                assert!(session.revoke());
                assert!(!session.revoke());
                assert_eq!(session.ready(), ready);
                serial::print(format_args!(
                    "M12CONTROL case={} ready={} cancelled={} repeat_cancel=0\n",
                    case, ready, cancelled
                ));
            }
            while let Some(event) = session.dispatch() {
                count += 1;
                if count <= 64 {
                    record(case, event);
                }
            }
            drop(session);
            assert_eq!(host.launch.stops(), [1, 1, 1, 0]);
            serial::print(format_args!(
                "M12STOP case={} stops=3 owner_caps={} reads={}\n",
                case,
                host.launch.stats().capabilities,
                host.reads
            ));
            turns = count;
        }
        assert_eq!(accounting(), baseline);
        assert_eq!(store.stats().capabilities + store.stats().payload_frames, 0);
        let peak = runtime(|state| state.parts().0.frames.peak());
        assert_eq!(peak, baseline.0 + 1);
        serial::print(format_args!(
            "M12FRAMES case={} turns={} before={} after={} peak={} tables={} mappings={} payload=0 capabilities=0\n",
            case,
            turns,
            baseline.0,
            accounting().0,
            peak,
            baseline.1,
            baseline.2
        ));
    }
    batch.clear();
    assert_eq!(batch.count() + batch.code_bytes(), 0);
    serial::print(format_args!(
        "M12CLEAN cases=3 code_bytes=0 tasks=0\nM12 PASS\n"
    ));
}
