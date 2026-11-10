//! Observe a live external session without exposing its authority host.
use super::*;
use shared_first_os::{
    launch::{DEMO, Launch},
    loader::Batch,
    scheduler::TaskHost,
    session::{InspectableHost, Resources, RevocableHost, Session},
};

struct World<'a> {
    launch: Launch<'a, Hardware, 64>,
    case: usize,
}
impl TaskHost for World<'_> {
    fn sum(&mut self, task: usize, handle: u64, offset: u64, length: u64) -> Result<u64, Fault> {
        reset_trace();
        let result = self.launch.sum(task, handle, offset, length);
        let observed = trace();
        serial::print(format_args!(
            "M13READ case={} task={} allowed={} length={} value={} frame={:#x} pages={}\n",
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
        result
    }
    fn stop(&mut self, task: usize) {
        self.launch.stop(task);
    }
}
impl RevocableHost for World<'_> {
    fn revoke(&mut self) -> bool {
        self.launch.revoke()
    }
}
impl InspectableHost for World<'_> {
    fn resources(&self) -> Resources {
        self.launch.resources()
    }
}
fn record(session: &Session<'_, '_, World<'_>>, case: usize, step: usize, action: &str) {
    let snapshot = session.inspect();
    assert_eq!(snapshot, session.inspect());
    assert_eq!(snapshot.execution, session.snapshot());
    assert_eq!(snapshot.execution.ready, session.ready());
    let execution = snapshot.execution;
    let resources = snapshot.resources;
    serial::print(format_args!(
        "M13SNAP case={} step={} action={} admitted={} ready={} granted={} remaining={} objects={} payload={} capabilities={} mappings={} revoked={}\n",
        case,
        step,
        action,
        execution.admitted,
        execution.ready,
        execution.granted_fuel,
        execution.remaining_fuel,
        resources.objects,
        resources.payload_frames,
        resources.capabilities,
        resources.mapping_pages,
        usize::from(resources.revoked)
    ));
    for (task, state) in execution.tasks.into_iter().enumerate() {
        if let Some(state) = state {
            serial::print(format_args!(
                "M13STATE case={} step={} task={} state={:?} fuel={}\n",
                case, step, task, state.state, state.remaining_fuel
            ));
        } else {
            serial::print(format_args!(
                "M13STATE case={} step={} task={} state=Empty fuel=0\n",
                case, step, task
            ));
        }
    }
}
pub(super) fn run(store: &mut Bench) {
    let baseline = accounting();
    let module = crate::boot::bytecode_module(2).expect("M13 retained input module");
    let mut batch = match Batch::load(module.bytes) {
        Ok(batch) => batch,
        Err(error) => {
            assert_eq!(accounting(), baseline);
            serial::print(format_args!(
                "M13REJECT format={:?} admitted=0 grants=0\n",
                error
            ));
            return;
        }
    };
    for case in 0..2 {
        runtime(|state| state.parts().0.frames.reset_peak());
        {
            let launch = Launch::new(store, &batch, DEMO, 7).unwrap();
            let arguments = launch.arguments();
            let mut host = World { launch, case };
            let mut session = Session::with_arguments(&batch, &mut host, 1, arguments).unwrap();
            record(&session, case, 0, "initial");
            let mut step = 0;
            while let Some(event) = session.dispatch() {
                step += 1;
                assert!(step <= 16, "M13 finite fixture");
                serial::print(format_args!(
                    "M13EVENT case={} step={} task={} result={:?} before={} after={}\n",
                    case, step, event.task, event.result, event.fuel_before, event.fuel_after
                ));
                record(&session, case, step, "dispatch");
                if case == 1 && step == 2 {
                    assert!(session.cancel(0));
                    assert!(!session.cancel(0));
                    step += 1;
                    record(&session, case, step, "cancel");
                    let before = session.snapshot();
                    assert!(session.revoke());
                    assert!(!session.revoke());
                    assert_eq!(before, session.snapshot());
                    assert_eq!(accounting(), baseline);
                    step += 1;
                    record(&session, case, step, "revoke");
                }
            }
            let terminal = session.inspect();
            for task in [0, 1, 2, 3, usize::MAX] {
                assert!(!session.cancel(task));
            }
            assert!(session.dispatch().is_none());
            assert_eq!(terminal, session.inspect());
            record(&session, case, step + 1, "stable");
            drop(session);
            assert_eq!(host.launch.stops(), [1, 1, 1, 0]);
        }
        assert_eq!(accounting(), baseline);
        assert_eq!(store.stats().payload_frames + store.stats().capabilities, 0);
        let peak = runtime(|state| state.parts().0.frames.peak());
        assert_eq!(peak, baseline.0 + 1);
        serial::print(format_args!(
            "M13CLEAN case={} stops=3 before={} after={} peak={} tables={} mappings={} payload=0 capabilities=0\n",
            case,
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
        "M13CHECK cases=2 scheduler_bytes={} session_bytes={} snapshot_bytes={} inspection_bytes={} code_bytes=0\nM13 PASS\n",
        size_of::<shared_first_os::scheduler::Scheduler>(),
        size_of::<Session<'_, '_, World<'_>>>(),
        size_of::<shared_first_os::scheduler::Snapshot>(),
        size_of::<shared_first_os::session::Inspection>()
    ));
}
