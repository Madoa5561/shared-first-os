//! M8 hardware integration; the existing M7 store/backend retains RO checking.
use super::*;
use shared_first_os::{
    sandbox::Slice,
    scheduler::{AdmissionError, Dispatch, MAX_BATCH_FUEL, Scheduler, TaskHost},
};

struct World<'a> {
    store: &'a mut Bench,
    stops: [usize; 4],
    sums: [u64; 4],
}
impl TaskHost for World<'_> {
    fn sum(&mut self, task: usize, handle: u64, offset: u64, length: u64) -> Result<u64, Fault> {
        let value = BoundHost {
            store: self.store,
            task,
        }
        .sum(handle, offset, length)?;
        self.sums[task] = value;
        Ok(value)
    }
    fn stop(&mut self, task: usize) {
        self.store.release_task(task).unwrap();
        self.stops[task] += 1;
    }
}
fn record(scenario: usize, event: Dispatch) {
    let (result, value) = match event.result {
        Ok(Slice::Expired) => (0, 0),
        Ok(Slice::Event(Outcome::Yield(value))) => (1, value),
        Ok(Slice::Event(Outcome::Halt(value))) => (2, value),
        Err(Fault::Fuel) => (3, 0),
        Err(Fault::Authority) => (4, 0),
        Err(Fault::Bounds) => (5, 0),
        other => panic!("M8 unexpected result: {other:?}"),
    };
    serial::print(format_args!(
        "M8DISPATCH scenario={} task={} result={} value={} before={} after={}\n",
        scenario, event.task, result, value, event.fuel_before, event.fuel_after
    ));
}
fn fairness(store: &mut Bench) {
    let looping = ins(6, 0, 0, 0, 0);
    let healthy = [ins(1, 0, 0, 0, 42), ins(0, 0, 0, 0, 0)];
    let yielding = [ins(7, 0, 0, 0, 0), ins(6, 0, 0, 0, 0)];
    let bounds = [ins(1, 1, 0, 0, 64), ins(3, 0, 1, 0, 0)];
    let mut scheduler = Scheduler::new(2).unwrap();
    let mut host = World {
        store,
        stops: [0; 4],
        sums: [0; 4],
    };
    for (code, fuel) in [
        (&looping[..], 5),
        (healthy.as_flattened(), 2),
        (yielding.as_flattened(), 3),
        (bounds.as_flattened(), 2),
    ] {
        scheduler
            .admit(Program::validate(code).unwrap(), fuel)
            .unwrap();
    }
    let mut turns = 0;
    while let Some(event) = scheduler.dispatch(&mut host) {
        turns += 1;
        assert!(turns <= 8);
        record(0, event);
    }
    assert_eq!(turns, 8);
    scheduler.shutdown(&mut host);
    assert_eq!(host.stops, [1; 4]);
    assert_eq!(scheduler.ready(), 0);
}
fn sharing(store: &mut Bench) {
    let owner = store.create_filled(0, 4096, 7).unwrap();
    let reader1 = store.grant(0, owner, 1, Rights::READ).unwrap();
    let reader2 = store.grant(0, owner, 2, Rights::READ).unwrap();
    // No explicit YIELD before forced suspension or the second read.
    let first = [
        ins(1, 1, 0, 0, reader1.encode()),
        ins(2, 0, 1, 2, 4096),
        ins(2, 0, 1, 2, 4096),
        ins(0, 0, 0, 0, 0),
    ];
    let second = [
        ins(1, 1, 0, 0, reader2.encode()),
        ins(2, 0, 1, 2, 4096),
        ins(2, 0, 1, 2, 4096),
        ins(0, 0, 0, 0, 0),
    ];
    let looping = ins(6, 0, 0, 0, 0);
    let foreign = [ins(1, 1, 0, 0, owner.encode()), ins(2, 0, 1, 2, 4096)];
    let mut scheduler = Scheduler::new(2).unwrap();
    let mut host = World {
        store,
        stops: [0; 4],
        sums: [0; 4],
    };
    for (code, fuel) in [
        (&looping[..], 5),
        (first.as_flattened(), 10000),
        (second.as_flattened(), 10000),
        (foreign.as_flattened(), 10000),
    ] {
        scheduler
            .admit(Program::validate(code).unwrap(), fuel)
            .unwrap();
    }
    record(1, scheduler.dispatch(&mut host).unwrap());
    let read1 = scheduler.dispatch(&mut host).unwrap();
    assert_eq!(read1.result, Ok(Slice::Expired));
    let backing1 = trace();
    record(1, read1);
    let read2 = scheduler.dispatch(&mut host).unwrap();
    assert_eq!(read2.result, Ok(Slice::Expired));
    let backing2 = trace();
    record(1, read2);
    assert_eq!((backing1.count, backing2.count), (1, 1));
    assert_eq!(backing1.frames[0], backing2.frames[0]);
    assert_eq!(host.sums, [0, 28672, 28672, 0]);
    // The ungranted task must fail while the object is still live.
    let foreign = scheduler.dispatch(&mut host).unwrap();
    assert_eq!(foreign.result, Err(Fault::Authority));
    assert_eq!(trace().count, 0);
    record(1, foreign);
    let stats = host.store.stats();
    assert_eq!(stats.payload_frames, 1);
    serial::print(format_args!(
        "M8SHARED frame1={:#x} frame2={:#x} payload={} capabilities={} mappings={} sum1={} sum2={}\n",
        backing1.frames[0],
        backing2.frames[0],
        stats.payload_frames,
        stats.capabilities,
        stats.mapping_pages,
        host.sums[1],
        host.sums[2]
    ));
    host.store.revoke(0, owner).unwrap();
    for expected in [
        Ok(Slice::Expired),
        Err(Fault::Authority),
        Err(Fault::Authority),
        Err(Fault::Fuel),
    ] {
        let event = scheduler.dispatch(&mut host).unwrap();
        assert_eq!(event.result, expected);
        assert_eq!(trace().count, 0);
        record(1, event);
    }
    assert_eq!(scheduler.dispatch(&mut host), None);
    scheduler.shutdown(&mut host);
    assert_eq!(host.stops, [1; 4]);
}
fn quotas_and_cancel(store: &mut Bench) {
    let owner = store.create_filled(0, 4096, 9).unwrap();
    store.grant(0, owner, 1, Rights::READ).unwrap();
    let halt = ins(0, 0, 0, 0, 0);
    let program = Program::validate(&halt).unwrap();
    let mut scheduler = Scheduler::new(1).unwrap();
    let mut host = World {
        store,
        stops: [0; 4],
        sums: [0; 4],
    };
    assert_eq!(scheduler.admit(program, 0), Err(AdmissionError::Fuel));
    assert_eq!(
        scheduler.admit(program, u64::MAX),
        Err(AdmissionError::Fuel)
    );
    assert_eq!(scheduler.admit(program, MAX_BATCH_FUEL - 1), Ok(0));
    assert_eq!(scheduler.admit(program, 2), Err(AdmissionError::Fuel));
    assert_eq!(scheduler.admit(program, 1), Ok(1));
    assert!(scheduler.cancel(0, &mut host));
    assert!(!scheduler.cancel(0, &mut host));
    assert!(!scheduler.cancel(usize::MAX, &mut host));
    assert_eq!(host.store.stats().payload_frames, 1);
    record(2, scheduler.dispatch(&mut host).unwrap());
    assert_eq!(host.store.stats().payload_frames, 0);
    assert_eq!(scheduler.admit(program, 1), Err(AdmissionError::Fuel));
    scheduler.shutdown(&mut host);
    assert_eq!(host.stops, [1, 1, 0, 0]);
    assert!(scheduler.dispatch(&mut host).is_none());
    let mut scheduler = Scheduler::new(64).unwrap();
    for task in 0..4 {
        assert_eq!(scheduler.admit(program, 1), Ok(task));
    }
    assert_eq!(scheduler.admit(program, 1), Err(AdmissionError::Capacity));
    scheduler.shutdown(&mut host);
    assert_eq!(scheduler.admit(program, 1), Err(AdmissionError::Capacity));
    assert!(matches!(Scheduler::new(0), Err(AdmissionError::Quantum)));
    assert!(matches!(Scheduler::new(65), Err(AdmissionError::Quantum)));
    assert_eq!(host.stops, [2, 2, 1, 1]);
    serial::print(format_args!(
        "M8QUOTAS zero=1 overflow=1 aggregate=1 capacity=1 quantum=1 no_refund=1 no_reuse=1 cancel=1 shutdown=1\n"
    ));
}
pub(super) fn run(store: &mut Bench) {
    let baseline = accounting();
    runtime(|state| state.parts().0.frames.reset_peak());
    serial::print(format_args!(
        "M8CONFIG capacity=4 quantum=2 max_quantum=64 max_batch_fuel={} scheduler_bytes={} host_bytes={}\n",
        MAX_BATCH_FUEL,
        size_of::<Scheduler>(),
        size_of::<World>()
    ));
    fairness(store);
    sharing(store);
    quotas_and_cancel(store);
    assert_eq!(accounting(), baseline);
    assert_eq!(store.stats().payload_frames + store.stats().capabilities, 0);
    serial::print(format_args!(
        "M8FRAMES before={} after={} tables_before={} tables_after={} mappings_before={} mappings_after={} payload=0 capabilities=0\n",
        baseline.0,
        accounting().0,
        baseline.1,
        accounting().1,
        baseline.2,
        accounting().2
    ));
    serial::print(format_args!(
        "M8CHECK dispatches=17 fairness=1 fault_isolation=1 shared=1 revocation=1 cleanup=1\nM8 PASS\n"
    ));
}
