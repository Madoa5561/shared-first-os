//! End-to-end launch and cleanup through the bounded public runner.
use super::*;
use shared_first_os::{
    launch::DEMO,
    loader::MAX_IMAGE,
    runner::{Completion, Report, RunOptions, execute},
};

pub(super) fn run(store: &mut Bench) {
    let module = crate::boot::bytecode_module(2).expect("M14 retained external module");
    let mut changed = [0; MAX_IMAGE];
    changed[..module.bytes.len()].copy_from_slice(module.bytes);
    let baseline = accounting();
    for case in 0..8 {
        runtime(|state| state.parts().0.frames.reset_peak());
        let owner = if case == 5 {
            Some(store.create_filled(0, 4096, 9).unwrap())
        } else {
            None
        };
        let mut options = RunOptions {
            quantum: 1,
            dispatch_limit: 16,
            manifest: DEMO,
            fill: 7,
        };
        options.dispatch_limit = match case {
            1 => 9,
            2 => 2,
            3 => 0,
            _ => 16,
        };
        if case == 4 {
            options.quantum = 0;
        }
        let image = if case == 6 {
            changed[8] = 2;
            &changed[..module.bytes.len()]
        } else if case == 7 {
            changed[8] = 1;
            changed[10] = 2;
            &changed[..176]
        } else {
            module.bytes
        };
        reset_trace();
        let report = execute(store, image, options);
        let observed = trace();
        assert_eq!(report.before, report.after);
        match report.result {
            Ok(completion) => {
                assert_eq!(completion.stops, [1, 1, 1, 0]);
                let execution = completion.execution;
                serial::print(format_args!(
                    "M14DONE case={} dispatches={} limited={} ready={} granted={} remaining={} stops=3\n",
                    case,
                    completion.dispatches,
                    usize::from(completion.limited),
                    execution.ready,
                    execution.granted_fuel,
                    execution.remaining_fuel
                ));
                for (task, state) in execution.tasks.into_iter().enumerate() {
                    if let Some(state) = state {
                        serial::print(format_args!(
                            "M14STATE case={} task={} state={:?} fuel={}\n",
                            case, task, state.state, state.remaining_fuel
                        ));
                    } else {
                        serial::print(format_args!(
                            "M14STATE case={} task={} state=Empty fuel=0\n",
                            case, task
                        ));
                    }
                }
            }
            Err(error) => serial::print(format_args!("M14START case={} error={:?}\n", case, error)),
        }
        serial::print(format_args!(
            "M14ACCOUNT case={} objects={} payload={} capabilities={} private={} mappings={} preserved=1\n",
            case,
            report.after.objects,
            report.after.payload_frames,
            report.after.capabilities,
            report.after.private_frames,
            report.after.mapping_pages
        ));
        let kept = if let Some(owner) = owner {
            let value = store.read_sum(0, owner, 0, 4096).unwrap();
            assert_eq!(value, 36864);
            store.release_task(0).unwrap();
            value
        } else {
            0
        };
        assert_eq!(accounting(), baseline);
        assert_eq!(store.stats().payload_frames + store.stats().capabilities, 0);
        let frame = if observed.count == 0 {
            0
        } else {
            observed.frames[0]
        };
        assert!(
            observed.frames[..observed.count]
                .iter()
                .all(|&value| value == frame)
        );
        let peak = runtime(|state| state.parts().0.frames.peak());
        serial::print(format_args!(
            "M14CLEAN case={} before={} after={} peak={} tables={} mappings={} reads={} frame={:#x} kept={}\n",
            case,
            baseline.0,
            accounting().0,
            peak,
            baseline.1,
            baseline.2,
            observed.count,
            frame,
            kept
        ));
    }
    serial::print(format_args!(
        "M14CHECK cases=8 max_dispatches={} report_bytes={} completion_bytes={}\nM14 PASS\n",
        shared_first_os::runner::MAX_DISPATCHES,
        size_of::<Report>(),
        size_of::<Completion>()
    ));
}
