//! External code receives only the grants selected by trusted launch policy.
use super::*;
use shared_first_os::{
    launch::{DEMO, Launch},
    loader::Batch,
    scheduler::TaskHost,
    session::Session,
};
struct World<'a> {
    launch: Launch<'a, Hardware, 64>,
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
                "M11READ task={} allowed={} length={} value={} frame={:#x} pages={}\n",
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
pub(super) fn run(store: &mut Bench) {
    let baseline = accounting();
    assert_eq!(store.stats().objects + store.stats().capabilities, 0);
    let module = match crate::boot::bytecode_module(1) {
        Ok(module) => module,
        Err(error) => {
            assert_eq!(accounting(), baseline);
            serial::print(format_args!(
                "M11REJECT transport={} admitted=0 grants=0\n",
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
        "M11MODULE bytes={} physical={:#x} retained_pages={} fingerprint={}\n",
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
                "M11REJECT format={:?} admitted=0 grants=0\n",
                error
            ));
            return;
        }
    };
    runtime(|state| state.parts().0.frames.reset_peak());
    let turns;
    {
        let launch = match Launch::new(store, &batch, DEMO, 7) {
            Ok(launch) => launch,
            Err(error) => {
                assert_eq!(accounting(), baseline);
                serial::print(format_args!(
                    "M11REJECT manifest={:?} admitted=0 grants=0\n",
                    error
                ));
                return;
            }
        };
        let arguments = launch.arguments();
        assert_eq!(launch.stats().payload_frames, 1);
        assert_eq!(launch.stats().capabilities, 3);
        serial::print(format_args!(
            "M11LAUNCH tasks={} readers=2 payload=1 capabilities=3 code_bytes={} fuel={} launch_bytes={}\n",
            batch.count(),
            batch.code_bytes(),
            batch.fuel(),
            size_of::<Launch<'_, Hardware, 64>>()
        ));
        let mut host = World { launch, reads: 0 };
        let mut session = Session::with_arguments(&batch, &mut host, 2, arguments).unwrap();
        let mut count = 0;
        while let Some(event) = session.dispatch() {
            count += 1;
            if count <= 64 {
                serial::print(format_args!(
                    "M11TASK task={} result={:?} before={} after={}\n",
                    event.task, event.result, event.fuel_before, event.fuel_after
                ));
            }
        }
        drop(session);
        assert_eq!(host.launch.stops(), [1, 1, 1, 0]);
        assert_eq!(host.launch.stats().capabilities, 1);
        turns = count;
        serial::print(format_args!(
            "M11STOP stops=3 owner_caps=1 reads={}\n",
            host.reads
        ));
    }
    batch.clear();
    assert_eq!(accounting(), baseline);
    assert_eq!(store.stats().capabilities + store.stats().payload_frames, 0);
    let peak = runtime(|state| state.parts().0.frames.peak());
    assert_eq!(peak, baseline.0 + 1);
    serial::print(format_args!(
        "M11CLEAN turns={} before={} after={} peak={} tables={} mappings={} capabilities=0 payload=0 code_bytes={}\nM11 PASS\n",
        turns,
        baseline.0,
        accounting().0,
        peak,
        baseline.1,
        baseline.2,
        batch.code_bytes()
    ));
}
