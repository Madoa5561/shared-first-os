//! Model corpus against real shared physical backing.
use super::*;
use shared_first_os::launch_model::{
    GENERATED_CASES, REJECTED_CASES, check_generated, check_rejected,
};
pub(super) fn run(store: &mut Bench) {
    let baseline = accounting();
    runtime(|state| state.parts().0.frames.reset_peak());
    let mut digest = 0u64;
    let mut reads = 0;
    for index in 0..GENERATED_CASES {
        reset_trace();
        digest = digest.wrapping_add(check_generated(store, index));
        let observed = trace();
        if observed.count != 0 {
            assert!(
                observed.frames[..observed.count]
                    .iter()
                    .all(|&frame| frame == observed.frames[0])
            );
        }
        reads += observed.count;
        assert_eq!(accounting(), baseline);
        assert_eq!(store.stats().payload_frames + store.stats().capabilities, 0);
    }
    let peak = runtime(|state| state.parts().0.frames.peak());
    assert_eq!(peak, baseline.0 + 1);
    runtime(|state| state.parts().0.frames.reset_peak());
    for index in 0..REJECTED_CASES {
        reset_trace();
        check_rejected(store, index);
        assert_eq!(trace().count, 0);
        assert_eq!(accounting(), baseline);
    }
    assert_eq!(runtime(|state| state.parts().0.frames.peak()), baseline.0);
    serial::print(format_args!(
        "LAUNCHRESULT generated={} rejected={} digest={} reads={} before={} after={} peak={} reject_peak={} tables={} mappings={} payload=0 capabilities=0\nLAUNCHMODEL PASS\n",
        GENERATED_CASES,
        REJECTED_CASES,
        digest,
        reads,
        baseline.0,
        accounting().0,
        peak,
        baseline.0,
        baseline.1,
        baseline.2
    ));
}
