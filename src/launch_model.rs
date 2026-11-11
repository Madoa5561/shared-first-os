//! Deterministic research corpus and independent reference execution model.
//! The model does not use Context, Scheduler, Session or the package parser.
use crate::{
    launch::ReadManifest,
    loader::MAGIC,
    object::{ObjectStore, ReadBackend},
    runner::{RunOptions, StartError, execute},
    sandbox::{Fault, instruction},
    scheduler::{Snapshot, TaskSnapshot, TaskState},
};

pub const GENERATED_CASES: usize = 240;
pub const REJECTED_CASES: usize = 264;
fn fuel(seed: usize, task: usize) -> u64 {
    2 + ((seed * 3 + task) % 11) as u64
}
fn image(seed: usize) -> [u8; 256] {
    let mut bytes = [0; 256];
    bytes[..8].copy_from_slice(&MAGIC);
    bytes[8] = 1;
    bytes[10] = 3;
    for task in 0..3 {
        let at = 16 + task * 80;
        bytes[at] = 64;
        bytes[at + 4] = fuel(seed, task) as u8;
        let halt = instruction(0, 0, 0, 0, 0);
        let words = match (seed + task) % 4 {
            0 => [
                instruction(1, 0, 0, 0, (seed + task) as u64),
                instruction(7, 0, 0, 0, 0),
                instruction(5, 0, 0, 0, 0),
                halt,
            ],
            1 => [
                instruction(2, 0, 1, 2, (seed % 4 + 1) as u64),
                instruction(7, 0, 0, 0, 0),
                instruction(2, 0, 1, 2, (seed % 4 + 1) as u64),
                halt,
            ],
            2 => [instruction(6, 0, 0, 0, 0), halt, halt, halt],
            _ => [
                instruction(1, 2, 0, 0, 64),
                instruction(3, 0, 2, 0, 0),
                instruction(7, 0, 0, 0, 0),
                halt,
            ],
        };
        for (index, word) in words.into_iter().enumerate() {
            bytes[at + 16 + index * 16..at + 32 + index * 16].copy_from_slice(&word);
        }
    }
    bytes
}

fn reference(seed: usize, quantum: u32, limit: usize) -> (Snapshot, usize, bool) {
    let mut tasks = core::array::from_fn(|task| {
        if task < 3 {
            Some(TaskSnapshot {
                state: TaskState::Ready,
                remaining_fuel: fuel(seed, task),
            })
        } else {
            None
        }
    });
    let mut positions = [0; 3];
    let mut values = [0; 3];
    let mut cursor = 0;
    let mut turns = 0;
    while turns < limit {
        let next = (0..3)
            .map(|offset| (cursor + offset) % 3)
            .find(|&task| tasks[task].unwrap().state == TaskState::Ready);
        let Some(task) = next else {
            break;
        };
        cursor = (task + 1) % 3;
        turns += 1;
        let status = tasks[task].as_mut().unwrap();
        for _ in 0..quantum {
            let kind = (seed + task) % 4;
            let pc = positions[task];
            let reading = kind == 1 && (pc == 0 || pc == 2);
            let cost = if reading { 2 + seed as u64 % 4 } else { 1 };
            if status.remaining_fuel < cost {
                status.state = TaskState::Faulted(Fault::Fuel);
                break;
            }
            status.remaining_fuel -= cost;
            positions[task] += 1;
            if kind == 2 {
                positions[task] = 0;
                continue;
            }
            if kind == 3 && pc == 1 {
                status.state = TaskState::Faulted(Fault::Bounds);
                break;
            }
            if reading {
                if seed & (1 << task) == 0 {
                    status.state = TaskState::Faulted(Fault::Authority);
                    break;
                }
                values[task] = (seed as u64 + 1) * (seed as u64 % 4 + 1);
            } else if pc == 0 && kind == 0 {
                values[task] = (seed + task) as u64;
            } else if pc == 2 && kind == 0 {
                values[task] *= 2;
            }
            if pc == 3 {
                status.state = TaskState::Halted(values[task]);
                break;
            }
            if pc == 1 && kind < 2 {
                break;
            }
        }
    }
    let limited = tasks
        .iter()
        .flatten()
        .any(|task| task.state == TaskState::Ready);
    for task in tasks.iter_mut().flatten() {
        if task.state == TaskState::Ready {
            task.state = TaskState::Cancelled;
        }
    }
    (
        Snapshot {
            admitted: 3,
            ready: 0,
            granted_fuel: (0..3).map(|task| fuel(seed, task)).sum(),
            remaining_fuel: tasks.iter().flatten().map(|task| task.remaining_fuel).sum(),
            tasks,
        },
        turns,
        limited,
    )
}

/// Returns a deterministic digest contribution after comparing every task field.
pub fn check_generated<B: ReadBackend, const N: usize>(
    store: &mut ObjectStore<B, N>,
    index: usize,
) -> u64 {
    assert!(index < GENERATED_CASES);
    let seed = index / 15;
    let quantum = [1, 2, 64][index / 5 % 3];
    let limit = [0, 1, 2, 7, 32][index % 5];
    let options = RunOptions {
        quantum,
        dispatch_limit: limit,
        manifest: ReadManifest {
            readers: core::array::from_fn(|task| seed & (1 << task) != 0),
        },
        fill: (seed + 1) as u8,
    };
    let report = execute(store, &image(seed), options);
    assert_eq!(report.before, report.after);
    let actual = report.result.unwrap();
    let (expected, turns, limited) = reference(seed, quantum, limit);
    assert_eq!(actual.execution, expected);
    assert_eq!((actual.dispatches, actual.limited), (turns, limited));
    assert_eq!(actual.stops, [1, 1, 1, 0]);
    let mut digest = turns as u64 + u64::from(limited) * 97;
    for task in actual.execution.tasks.into_iter().flatten() {
        let state = match task.state {
            TaskState::Halted(value) => 1000 + value,
            TaskState::Faulted(Fault::Authority) => 2000,
            TaskState::Faulted(Fault::Bounds) => 3000,
            TaskState::Faulted(Fault::Fuel) => 4000,
            TaskState::Cancelled => 5000,
            _ => panic!("unexpected model terminal state"),
        };
        digest = digest * 31 + state + task.remaining_fuel;
    }
    digest
}

pub fn check_rejected<B: ReadBackend, const N: usize>(store: &mut ObjectStore<B, N>, index: usize) {
    assert!(index < REJECTED_CASES);
    let mut bytes = image(7);
    let length = if index < 256 {
        index
    } else {
        let (offset, value) = [
            (0, 0),
            (8, 2),
            (10, 0),
            (12, 1),
            (16, 0),
            (20, 0),
            (24, 1),
            (240, 255),
        ][index - 256];
        bytes[offset] = value;
        256
    };
    let report = execute(
        store,
        &bytes[..length],
        RunOptions {
            quantum: 1,
            dispatch_limit: 32,
            manifest: ReadManifest { readers: [true; 3] },
            fill: 7,
        },
    );
    assert!(matches!(report.result, Err(StartError::Load(_))));
    assert_eq!(report.before, report.after);
}
