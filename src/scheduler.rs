//! Fixed, non-reusable batch slots for restricted bytecode only. No heap/unsafe.
use crate::sandbox::{Context, Fault, Host, Outcome, Program, Slice};
use core::num::NonZeroU32;

pub const CAPACITY: usize = 4;
pub const MAX_QUANTUM: u32 = 64;
pub const MAX_BATCH_FUEL: u64 = 2 * 1024 * 1024;

/// Trusted task-local authority adapter. `stop` must release that task's
/// capabilities/mappings before returning; it cannot reenter the scheduler.
/// Each batch exclusively owns its task tables. Slots are never reused in a batch.
pub trait TaskHost {
    fn sum(&mut self, task: usize, handle: u64, offset: u64, length: u64) -> Result<u64, Fault>;
    fn stop(&mut self, task: usize);
}
struct Bound<'a, H>(&'a mut H, usize);
impl<H: TaskHost> Host for Bound<'_, H> {
    fn sum(&mut self, handle: u64, offset: u64, length: u64) -> Result<u64, Fault> {
        self.0.sum(self.1, handle, offset, length)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionError {
    Quantum,
    Capacity,
    Fuel,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dispatch {
    pub task: usize,
    pub result: Result<Slice, Fault>,
    pub fuel_before: u64,
    pub fuel_after: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Halted(u64),
    Faulted(Fault),
    Cancelled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskSnapshot {
    pub state: TaskState,
    pub remaining_fuel: u64,
}
/// Owned diagnostic copy, never authority. Remaining fuel includes terminal
/// tasks' unused budgets; it cannot be refunded or granted to another task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub tasks: [Option<TaskSnapshot>; CAPACITY],
    pub admitted: usize,
    pub ready: usize,
    pub granted_fuel: u64,
    pub remaining_fuel: u64,
}
struct Task<'a> {
    context: Context<'a>,
    state: TaskState,
}
pub struct Scheduler<'a> {
    tasks: [Option<Task<'a>>; CAPACITY],
    admitted: usize,
    granted_fuel: u64,
    cursor: usize,
    quantum: NonZeroU32,
}
impl<'a> Scheduler<'a> {
    pub fn new(quantum: u32) -> Result<Self, AdmissionError> {
        if quantum == 0 || quantum > MAX_QUANTUM {
            return Err(AdmissionError::Quantum);
        }
        Ok(Self {
            tasks: [const { None }; CAPACITY],
            admitted: 0,
            granted_fuel: 0,
            cursor: 0,
            quantum: NonZeroU32::new(quantum).unwrap(),
        })
    }
    /// Admission grants a lifetime budget, never a per-dispatch refill.
    /// Rejected admission changes neither slots nor cumulative budget.
    pub fn admit(&mut self, program: Program<'a>, fuel: u64) -> Result<usize, AdmissionError> {
        self.admit_with_argument(program, fuel, 0)
    }
    /// Kernel-supplied r1, isolated to the newly admitted context.
    pub fn admit_with_argument(
        &mut self,
        program: Program<'a>,
        fuel: u64,
        argument: u64,
    ) -> Result<usize, AdmissionError> {
        if self.admitted == CAPACITY {
            return Err(AdmissionError::Capacity);
        }
        if fuel == 0 || fuel > MAX_BATCH_FUEL - self.granted_fuel {
            return Err(AdmissionError::Fuel);
        }
        let task = self.admitted;
        self.tasks[task] = Some(Task {
            context: Context::with_argument(program, fuel, argument),
            state: TaskState::Ready,
        });
        self.admitted += 1;
        self.granted_fuel += fuel;
        Ok(task)
    }
    pub fn granted_fuel(&self) -> u64 {
        self.granted_fuel
    }
    pub fn ready(&self) -> usize {
        self.tasks
            .iter()
            .flatten()
            .filter(|task| task.state == TaskState::Ready)
            .count()
    }
    pub fn snapshot(&self) -> Snapshot {
        let tasks = core::array::from_fn(|index| {
            self.tasks[index].as_ref().map(|task| TaskSnapshot {
                state: task.state,
                remaining_fuel: task.context.remaining(),
            })
        });
        Snapshot {
            remaining_fuel: tasks.iter().flatten().map(|task| task.remaining_fuel).sum(),
            tasks,
            admitted: self.admitted,
            ready: self.ready(),
            granted_fuel: self.granted_fuel,
        }
    }
    /// At most CAPACITY slots scanned, then at most quantum instructions.
    /// A ready task waits behind at most CAPACITY-1 other dispatches.
    pub fn dispatch(&mut self, host: &mut impl TaskHost) -> Option<Dispatch> {
        for _ in 0..CAPACITY {
            let id = self.cursor;
            self.cursor = (self.cursor + 1) % CAPACITY;
            let Some(task) = &mut self.tasks[id] else {
                continue;
            };
            if task.state != TaskState::Ready {
                continue;
            }
            let fuel_before = task.context.remaining();
            let result = task.context.run_slice(&mut Bound(host, id), self.quantum);
            task.state = match result {
                Ok(Slice::Event(Outcome::Halt(value))) => TaskState::Halted(value),
                Err(fault) => TaskState::Faulted(fault),
                _ => TaskState::Ready,
            };
            if task.state != TaskState::Ready {
                host.stop(id);
            }
            return Some(Dispatch {
                task: id,
                result,
                fuel_before,
                fuel_after: task.context.remaining(),
            });
        }
        None
    }
    /// Cancellation is host-only, idempotent and does not refund admission fuel.
    pub fn cancel(&mut self, id: usize, host: &mut impl TaskHost) -> bool {
        let Some(Some(task)) = self.tasks.get_mut(id) else {
            return false;
        };
        if task.state != TaskState::Ready {
            return false;
        }
        task.state = TaskState::Cancelled;
        host.stop(id);
        true
    }
    /// Explicit shutdown is required before discarding a live batch: Drop cannot
    /// access the external authority host. HALT/fault/cancel already call stop.
    pub fn shutdown(&mut self, host: &mut impl TaskHost) {
        for id in 0..CAPACITY {
            self.cancel(id, host);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::{MAX_BYTES, instruction as ins};
    #[derive(Default)]
    struct World {
        calls: [usize; 4],
        stops: [usize; 4],
        deny: bool,
    }
    impl TaskHost for World {
        fn sum(&mut self, task: usize, _: u64, _: u64, length: u64) -> Result<u64, Fault> {
            self.calls[task] += 1;
            if self.deny {
                Err(Fault::Authority)
            } else {
                Ok(length)
            }
        }
        fn stop(&mut self, task: usize) {
            self.stops[task] += 1;
        }
    }
    #[test]
    fn snapshots_retain_terminal_reasons_and_unused_fuel() {
        let halt = ins(0, 0, 0, 0, 0);
        let read = ins(2, 0, 0, 0, 1);
        let looping = ins(6, 0, 0, 0, 0);
        let mut scheduler = Scheduler::new(1).unwrap();
        let mut host = World {
            deny: true,
            ..World::default()
        };
        for code in [&halt, &read, &looping] {
            scheduler
                .admit(Program::validate(code).unwrap(), 5)
                .unwrap();
        }
        let initial = scheduler.snapshot();
        assert_eq!(
            (initial.admitted, initial.ready, initial.remaining_fuel),
            (3, 3, 15)
        );
        assert_eq!(initial.tasks[3], None);
        scheduler.dispatch(&mut host).unwrap();
        scheduler.dispatch(&mut host).unwrap();
        assert!(scheduler.cancel(2, &mut host));
        let terminal = scheduler.snapshot();
        assert_eq!(
            terminal.tasks,
            [
                Some(TaskSnapshot {
                    state: TaskState::Halted(0),
                    remaining_fuel: 4
                }),
                Some(TaskSnapshot {
                    state: TaskState::Faulted(Fault::Authority),
                    remaining_fuel: 3
                }),
                Some(TaskSnapshot {
                    state: TaskState::Cancelled,
                    remaining_fuel: 5
                }),
                None,
            ]
        );
        assert_eq!(
            (
                terminal.ready,
                terminal.granted_fuel,
                terminal.remaining_fuel
            ),
            (0, 15, 12)
        );
        for task in [0, 1, 2, 3, usize::MAX] {
            assert!(!scheduler.cancel(task, &mut host));
        }
        scheduler.shutdown(&mut host);
        assert_eq!(scheduler.dispatch(&mut host), None);
        assert_eq!(scheduler.snapshot(), terminal);
        assert_eq!(host.stops, [1, 1, 1, 0]);
    }
    #[test]
    fn observation_and_detached_copy_do_not_change_dispatch_or_fuel() {
        let words = [ins(1, 0, 0, 0, 7), ins(7, 0, 0, 0, 0), ins(0, 0, 0, 0, 0)];
        let looping = ins(6, 0, 0, 0, 0);
        let mut observed = Scheduler::new(1).unwrap();
        let mut control = Scheduler::new(1).unwrap();
        for scheduler in [&mut observed, &mut control] {
            scheduler
                .admit(Program::validate(words.as_flattened()).unwrap(), 4)
                .unwrap();
            scheduler
                .admit(Program::validate(&looping).unwrap(), 2)
                .unwrap();
        }
        let mut observed_host = World::default();
        let mut control_host = World::default();
        loop {
            let snapshot = observed.snapshot();
            assert_eq!(snapshot, observed.snapshot());
            let mut detached = snapshot;
            detached.remaining_fuel = u64::MAX;
            detached.tasks.fill(None);
            assert_ne!(detached, observed.snapshot());
            let event = observed.dispatch(&mut observed_host);
            assert_eq!(event, control.dispatch(&mut control_host));
            if let Some(event) = event {
                if matches!(
                    event.result,
                    Ok(Slice::Expired | Slice::Event(Outcome::Yield(_)))
                ) {
                    assert_eq!(
                        observed.snapshot().tasks[event.task].unwrap().state,
                        TaskState::Ready
                    );
                }
            } else {
                break;
            }
        }
        let snapshot = observed.snapshot();
        assert_eq!(snapshot, control.snapshot());
        assert_eq!(snapshot.tasks[0].unwrap().state, TaskState::Halted(7));
        assert_eq!(
            snapshot.tasks[1].unwrap().state,
            TaskState::Faulted(Fault::Fuel)
        );
        assert_eq!(snapshot.remaining_fuel, 1);
        assert_eq!(observed_host.stops, control_host.stops);
        assert_eq!(core::mem::size_of::<Scheduler<'_>>(), 640);
    }
    #[test]
    fn rejected_admission_preserves_snapshot() {
        let halt = ins(0, 0, 0, 0, 0);
        let program = Program::validate(&halt).unwrap();
        let mut scheduler = Scheduler::new(1).unwrap();
        scheduler.admit(program, MAX_BATCH_FUEL).unwrap();
        let before = scheduler.snapshot();
        assert_eq!(scheduler.admit(program, 1), Err(AdmissionError::Fuel));
        assert_eq!(scheduler.snapshot(), before);
        let mut scheduler = Scheduler::new(1).unwrap();
        for _ in 0..CAPACITY {
            scheduler.admit(program, 1).unwrap();
        }
        let before = scheduler.snapshot();
        assert_eq!(scheduler.admit(program, 1), Err(AdmissionError::Capacity));
        assert_eq!(scheduler.snapshot(), before);
    }
    #[test]
    fn non_yielding_loop_cannot_starve_peer_and_terminal_is_not_restarted() {
        let looping = ins(6, 0, 0, 0, 0);
        let halt = ins(0, 0, 0, 0, 0);
        let mut scheduler = Scheduler::new(2).unwrap();
        let mut host = World::default();
        scheduler
            .admit(Program::validate(&looping).unwrap(), 5)
            .unwrap();
        scheduler
            .admit(Program::validate(&halt).unwrap(), 1)
            .unwrap();
        let first = scheduler.dispatch(&mut host).unwrap();
        assert_eq!(
            (first.task, first.result, first.fuel_after),
            (0, Ok(Slice::Expired), 3)
        );
        assert_eq!(
            scheduler.dispatch(&mut host).unwrap().result,
            Ok(Slice::Event(Outcome::Halt(0)))
        );
        assert_eq!(scheduler.dispatch(&mut host).unwrap().fuel_after, 1);
        assert_eq!(
            scheduler.dispatch(&mut host).unwrap().result,
            Err(Fault::Fuel)
        );
        assert_eq!(scheduler.dispatch(&mut host), None);
        scheduler.shutdown(&mut host);
        assert_eq!(host.stops, [1, 1, 0, 0]);
    }
    #[test]
    fn all_four_tasks_rotate_and_yield_never_refills() {
        let words = [ins(7, 0, 0, 0, 0), ins(6, 0, 0, 0, 0)];
        let program = Program::validate(words.as_flattened()).unwrap();
        let mut scheduler = Scheduler::new(4).unwrap();
        let mut host = World::default();
        for task in 0..4 {
            assert_eq!(scheduler.admit(program, 3), Ok(task));
        }
        for _ in 0..2 {
            for task in 0..4 {
                let event = scheduler.dispatch(&mut host).unwrap();
                assert_eq!(event.task, task);
                assert_eq!(event.result, Ok(Slice::Event(Outcome::Yield(0))));
                assert!(event.fuel_after < event.fuel_before);
            }
        }
        for task in 0..4 {
            let event = scheduler.dispatch(&mut host).unwrap();
            assert_eq!((event.task, event.result), (task, Err(Fault::Fuel)));
        }
        assert_eq!(scheduler.ready(), 0);
        assert_eq!(host.stops, [1; 4]);
    }
    #[test]
    fn admission_limits_are_atomic_and_not_refunded() {
        assert!(matches!(Scheduler::new(0), Err(AdmissionError::Quantum)));
        assert!(matches!(Scheduler::new(65), Err(AdmissionError::Quantum)));
        let halt = ins(0, 0, 0, 0, 0);
        let program = Program::validate(&halt).unwrap();
        let mut scheduler = Scheduler::new(1).unwrap();
        let mut host = World::default();
        for fuel in [0, u64::MAX, MAX_BATCH_FUEL + 1] {
            assert_eq!(scheduler.admit(program, fuel), Err(AdmissionError::Fuel));
        }
        assert_eq!(scheduler.granted_fuel(), 0);
        assert_eq!(scheduler.admit(program, MAX_BATCH_FUEL), Ok(0));
        scheduler.dispatch(&mut host).unwrap();
        assert_eq!(scheduler.admit(program, 1), Err(AdmissionError::Fuel));
        assert_eq!(scheduler.granted_fuel(), MAX_BATCH_FUEL);
        let mut scheduler = Scheduler::new(64).unwrap();
        for id in 0..4 {
            assert_eq!(scheduler.admit(program, 1), Ok(id));
        }
        scheduler.shutdown(&mut host);
        assert_eq!(scheduler.admit(program, 1), Err(AdmissionError::Capacity));
    }
    #[test]
    fn sliced_bulk_call_is_charged_before_host_and_uses_bound_identity() {
        let words = [ins(2, 0, 1, 2, MAX_BYTES), ins(0, 0, 0, 0, 0)];
        let program = Program::validate(words.as_flattened()).unwrap();
        let mut scheduler = Scheduler::new(1).unwrap();
        let mut host = World::default();
        scheduler.admit(program, MAX_BYTES).unwrap();
        scheduler.admit(program, MAX_BYTES + 2).unwrap();
        assert_eq!(
            scheduler.dispatch(&mut host).unwrap().result,
            Err(Fault::Fuel)
        );
        let event = scheduler.dispatch(&mut host).unwrap();
        assert_eq!((event.result, event.fuel_after), (Ok(Slice::Expired), 1));
        assert_eq!(host.calls, [0, 1, 0, 0]);
        assert_eq!(
            scheduler.dispatch(&mut host).unwrap().result,
            Ok(Slice::Event(Outcome::Halt(MAX_BYTES)))
        );
    }
    #[test]
    fn authority_is_rechecked_after_forced_suspension() {
        let words = [ins(2, 0, 1, 2, 1), ins(2, 0, 1, 2, 1)];
        let mut scheduler = Scheduler::new(1).unwrap();
        let mut host = World::default();
        scheduler
            .admit(Program::validate(words.as_flattened()).unwrap(), 4)
            .unwrap();
        assert_eq!(
            scheduler.dispatch(&mut host).unwrap().result,
            Ok(Slice::Expired)
        );
        host.deny = true;
        assert_eq!(
            scheduler.dispatch(&mut host).unwrap().result,
            Err(Fault::Authority)
        );
        assert_eq!(host.stops, [1, 0, 0, 0]);
        assert_eq!(scheduler.dispatch(&mut host), None);
    }
    #[test]
    fn cancellation_shutdown_and_bounds_fault_clean_once() {
        let words = [ins(1, 1, 0, 0, 64), ins(3, 0, 1, 0, 0)];
        let program = Program::validate(words.as_flattened()).unwrap();
        let mut scheduler = Scheduler::new(2).unwrap();
        let mut host = World::default();
        for _ in 0..3 {
            scheduler.admit(program, 10).unwrap();
        }
        assert!(!scheduler.cancel(usize::MAX, &mut host));
        assert!(!scheduler.cancel(3, &mut host));
        assert!(scheduler.cancel(1, &mut host));
        assert!(!scheduler.cancel(1, &mut host));
        assert_eq!(
            scheduler.dispatch(&mut host).unwrap().result,
            Err(Fault::Bounds)
        );
        scheduler.shutdown(&mut host);
        scheduler.shutdown(&mut host);
        assert_eq!(host.stops, [1, 1, 1, 0]);
        assert_eq!(scheduler.dispatch(&mut host), None);
    }
}
