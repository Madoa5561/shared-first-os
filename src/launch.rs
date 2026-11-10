//! Trusted, fixed three-program read manifest. Packages cannot select grants.
use crate::{
    loader::Batch,
    object::{Error, Handle, ObjectStore, ReadBackend, Rights, Stats, TASKS},
    sandbox::Fault,
    scheduler::TaskHost,
    session::RevocableHost,
};

pub const PROGRAMS: usize = 3;
const OWNER: usize = 3;
#[derive(Clone, Copy)]
pub struct ReadManifest {
    pub readers: [bool; PROGRAMS],
}
pub const DEMO: ReadManifest = ReadManifest {
    readers: [true, true, false],
};
#[derive(Debug, PartialEq, Eq)]
pub enum LaunchError {
    Count,
    Busy,
    Object(Error),
}

/// Exclusively owns a previously empty store until Drop. Preparation errors also
/// release everything acquired. As with Session, abort/forget do not run Drop.
pub struct Launch<'a, B: ReadBackend, const N: usize> {
    store: &'a mut ObjectStore<B, N>,
    arguments: [u64; TASKS],
    active: [bool; TASKS],
    stops: [usize; TASKS],
    owner: Option<Handle>,
}
impl<'a, B: ReadBackend, const N: usize> Launch<'a, B, N> {
    pub fn new(
        store: &'a mut ObjectStore<B, N>,
        batch: &Batch,
        manifest: ReadManifest,
        fill: u8,
    ) -> Result<Self, LaunchError> {
        if batch.count() != PROGRAMS {
            return Err(LaunchError::Count);
        }
        let stats = store.stats();
        if stats.objects != 0 || stats.capabilities != 0 || stats.mapping_pages != 0 {
            return Err(LaunchError::Busy);
        }
        let mut launch = Self {
            store,
            arguments: [0; TASKS],
            active: [true; TASKS],
            stops: [0; TASKS],
            owner: None,
        };
        let owner = launch
            .store
            .create_filled(OWNER, 4096, fill)
            .map_err(LaunchError::Object)?;
        launch.owner = Some(owner);
        for (task, allowed) in manifest.readers.into_iter().enumerate() {
            if allowed {
                launch.arguments[task] = launch
                    .store
                    .grant(OWNER, owner, task, Rights::READ)
                    .map_err(LaunchError::Object)?
                    .encode();
            }
        }
        Ok(launch)
    }
    pub fn arguments(&self) -> [u64; TASKS] {
        self.arguments
    }
    pub fn stats(&self) -> Stats {
        self.store.stats()
    }
    pub fn stops(&self) -> [usize; TASKS] {
        self.stops
    }
    fn release(&mut self, task: usize) {
        if self.active[task] {
            self.store
                .release_task(task)
                .expect("owned launch task teardown");
            self.active[task] = false;
            self.stops[task] += 1;
        }
    }
}
impl<B: ReadBackend, const N: usize> TaskHost for Launch<'_, B, N> {
    fn sum(&mut self, task: usize, handle: u64, offset: u64, length: u64) -> Result<u64, Fault> {
        if task >= PROGRAMS || !self.active[task] {
            return Err(Fault::Authority);
        }
        let handle = Handle::decode(handle).map_err(|_| Fault::Authority)?;
        self.store
            .read_sum(task, handle, offset, length)
            .map_err(|error| match error {
                Error::InvalidAddress | Error::Overflow => Fault::Bounds,
                _ => Fault::Authority,
            })
    }
    fn stop(&mut self, task: usize) {
        assert!(task < PROGRAMS, "scheduler cannot stop launch owner");
        self.release(task);
    }
}
impl<B: ReadBackend, const N: usize> Drop for Launch<'_, B, N> {
    fn drop(&mut self) {
        for task in 0..TASKS {
            self.release(task);
        }
    }
}
impl<B: ReadBackend, const N: usize> RevocableHost for Launch<'_, B, N> {
    fn revoke(&mut self) -> bool {
        let Some(owner) = self.owner.take() else {
            return false;
        };
        self.store
            .revoke(OWNER, owner)
            .expect("owned launch object revocation");
        self.arguments.fill(0);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        loader::MAGIC,
        object::{Backend, BackendError},
        sandbox::{Outcome, Slice, instruction},
        session::Session,
    };
    #[derive(Default)]
    struct Memory {
        live: bool,
        sealed: bool,
        byte: u8,
        fail: bool,
        seal_fail: bool,
    }
    impl Backend for Memory {
        fn allocate(&mut self, data: &[u8]) -> Option<u64> {
            if self.fail || self.live {
                return None;
            }
            assert_eq!(data.len(), 4096);
            self.live = true;
            self.byte = data[0];
            Some(4096)
        }
        fn seal(&mut self, _: u64) -> Result<(), BackendError> {
            if self.seal_fail {
                return Err(BackendError::Conflict);
            }
            self.sealed = true;
            Ok(())
        }
        fn destroy(&mut self, _: u64) {
            assert!(self.live);
            self.live = false;
            self.sealed = false;
        }
        fn valid_range(&self, _: usize, _: u64, _: usize) -> bool {
            true
        }
        fn map_readonly(&mut self, _: usize, _: u64, _: u64) -> Result<(), BackendError> {
            panic!("no mappings")
        }
        fn unmap(&mut self, _: usize, _: u64) -> u64 {
            panic!("no mappings")
        }
    }
    impl ReadBackend for Memory {
        fn sum_page(&self, frame: u64, offset: usize, length: usize) -> u64 {
            assert!(self.live && self.sealed && frame == 4096 && offset + length <= 4096);
            u64::from(self.byte) * length as u64
        }
    }
    fn batch() -> Batch {
        let mut bytes = [0; 160];
        bytes[..8].copy_from_slice(&MAGIC);
        bytes[8] = 1;
        bytes[10] = 3;
        for task in 0..3 {
            let at = 16 + task * 48;
            bytes[at] = 32;
            bytes[at + 4] = 4;
            bytes[at + 16..at + 32].copy_from_slice(&instruction(5, 0, 1, 0, 0));
            bytes[at + 32..at + 48].copy_from_slice(&instruction(0, 0, 0, 0, 0));
        }
        Batch::load(&bytes).unwrap()
    }
    #[test]
    fn task_local_read_only_grants_reject_foreign_stale_and_bounds() {
        let batch = batch();
        let mut store = ObjectStore::<_, 1>::new(Memory::default());
        let old;
        {
            let mut launch = Launch::new(&mut store, &batch, DEMO, 7).unwrap();
            let args = launch.arguments();
            old = args[0];
            assert_eq!(launch.stats().payload_frames, 1);
            assert_eq!(launch.sum(0, args[0], 0, 4096), Ok(28672));
            assert_eq!(launch.sum(1, args[1], 0, 4096), Ok(28672));
            assert_eq!(launch.sum(2, args[0], 0, 1), Err(Fault::Authority));
            assert_eq!(launch.sum(3, args[0], 0, 1), Err(Fault::Authority));
            assert_eq!(launch.sum(0, args[0], 4096, 1), Err(Fault::Bounds));
            let handle = Handle::decode(args[0]).unwrap();
            assert_eq!(
                launch.store.grant(0, handle, 2, Rights::READ),
                Err(Error::Denied)
            );
            assert_eq!(
                launch.store.map(0, handle, 0x1000, Rights::READ),
                Err(Error::Denied)
            );
            launch.stop(0);
            launch.stop(0);
            assert_eq!(launch.stops(), [1, 0, 0, 0]);
            assert_eq!(launch.sum(0, args[0], 0, 1), Err(Fault::Authority));
        }
        assert_eq!(store.stats().payload_frames, 0);
        let mut next = Launch::new(&mut store, &batch, DEMO, 9).unwrap();
        assert_ne!(next.arguments()[0], old);
        assert_eq!(next.sum(0, old, 0, 1), Err(Fault::Authority));
    }
    #[test]
    fn session_receives_separate_arguments_and_cleanup_keeps_only_owner() {
        assert_eq!(core::mem::size_of::<Launch<'_, Memory, 1>>(), 88);
        let batch = batch();
        let mut store = ObjectStore::<_, 1>::new(Memory::default());
        {
            let mut launch = Launch::new(&mut store, &batch, DEMO, 7).unwrap();
            let args = [11, 22, 33, 0];
            let mut session = Session::with_arguments(&batch, &mut launch, 2, args).unwrap();
            for (task, value) in args.into_iter().enumerate().take(3) {
                let event = session.dispatch().unwrap();
                assert_eq!(event.task, task);
                assert_eq!(event.result, Ok(Slice::Event(Outcome::Halt(value))));
            }
            drop(session);
            assert_eq!(launch.stops(), [1, 1, 1, 0]);
            assert_eq!(launch.stats().capabilities, 1);
        }
        assert_eq!(store.stats().objects + store.stats().payload_frames, 0);
    }
    #[test]
    fn invalid_batch_busy_store_and_allocation_failure_are_atomic() {
        let mut batch = batch();
        let mut store = ObjectStore::<_, 1>::new(Memory::default());
        let owner = store.create_filled(0, 4096, 1).unwrap();
        assert!(matches!(
            Launch::new(&mut store, &batch, DEMO, 7),
            Err(LaunchError::Busy)
        ));
        assert_eq!(store.read_sum(0, owner, 0, 1), Ok(1));
        store.release_task(0).unwrap();
        store.backend_mut().fail = true;
        assert!(matches!(
            Launch::new(&mut store, &batch, DEMO, 7),
            Err(LaunchError::Object(_))
        ));
        assert_eq!(store.stats().capabilities + store.stats().payload_frames, 0);
        store.backend_mut().fail = false;
        store.backend_mut().seal_fail = true;
        assert!(matches!(
            Launch::new(&mut store, &batch, DEMO, 7),
            Err(LaunchError::Object(_))
        ));
        assert!(!store.backend().live);
        batch.clear();
        assert!(matches!(
            Launch::new(&mut store, &batch, DEMO, 7),
            Err(LaunchError::Count)
        ));
    }
    #[test]
    fn rejected_session_and_early_drop_release_prepared_grants() {
        let batch = batch();
        let mut store = ObjectStore::<_, 1>::new(Memory::default());
        {
            let mut launch = Launch::new(&mut store, &batch, DEMO, 7).unwrap();
            let args = launch.arguments();
            assert!(Session::with_arguments(&batch, &mut launch, 0, args).is_err());
        }
        assert_eq!(store.stats().payload_frames, 0);
        {
            let mut launch = Launch::new(&mut store, &batch, DEMO, 7).unwrap();
            let args = launch.arguments();
            drop(Session::with_arguments(&batch, &mut launch, 2, args).unwrap());
            assert_eq!(launch.stops(), [1, 1, 1, 0]);
        }
        assert_eq!(store.stats().capabilities + store.stats().payload_frames, 0);
    }
    #[test]
    fn trusted_manifest_selects_readers_without_ambient_rights() {
        let batch = batch();
        let mut store = ObjectStore::<_, 1>::new(Memory::default());
        let mut launch = Launch::new(
            &mut store,
            &batch,
            ReadManifest {
                readers: [false, true, true],
            },
            5,
        )
        .unwrap();
        let args = launch.arguments();
        assert_eq!(args[0], 0);
        assert_eq!(launch.sum(0, args[1], 0, 1), Err(Fault::Authority));
        assert_eq!(launch.sum(1, args[1], 0, 1), Ok(5));
        assert_eq!(launch.sum(2, args[2], 0, 1), Ok(5));
    }
    fn read_batch() -> Batch {
        let mut bytes = [0; 256];
        bytes[..8].copy_from_slice(&MAGIC);
        bytes[8] = 1;
        bytes[10] = 3;
        for task in 0..3 {
            let at = 16 + task * 80;
            bytes[at] = 64;
            bytes[at + 4] = 8;
            for (index, word) in [
                instruction(2, 0, 1, 2, 1),
                instruction(7, 0, 0, 0, 0),
                instruction(2, 0, 1, 2, 1),
                instruction(0, 0, 0, 0, 0),
            ]
            .into_iter()
            .enumerate()
            {
                bytes[at + 16 + index * 16..at + 32 + index * 16].copy_from_slice(&word);
            }
        }
        Batch::load(&bytes).unwrap()
    }
    #[test]
    fn session_revocation_denies_resumed_reads_without_refilling_fuel() {
        let batch = read_batch();
        let mut store = ObjectStore::<_, 1>::new(Memory::default());
        {
            let mut launch = Launch::new(&mut store, &batch, DEMO, 7).unwrap();
            let args = launch.arguments();
            let mut session = Session::with_arguments(&batch, &mut launch, 2, args).unwrap();
            for task in 0..2 {
                let event = session.dispatch().unwrap();
                assert_eq!(event.task, task);
                assert_eq!(event.result, Ok(Slice::Event(Outcome::Yield(7))));
                assert_eq!(event.fuel_after, 5);
            }
            assert!(session.revoke());
            assert!(!session.revoke());
            assert_eq!(session.ready(), 3);
            for (task, fuel) in [(2, 6), (0, 3), (1, 3)] {
                let event = session.dispatch().unwrap();
                assert_eq!(event.task, task);
                assert_eq!(event.result, Err(Fault::Authority));
                assert_eq!(event.fuel_after, fuel);
            }
            assert!(session.dispatch().is_none());
            drop(session);
            assert_eq!(launch.arguments(), [0; TASKS]);
            assert_eq!(launch.stops(), [1, 1, 1, 0]);
            assert_eq!(
                launch.stats().capabilities + launch.stats().payload_frames,
                0
            );
        }
        assert!(!store.backend().live);
    }
    #[test]
    fn cancel_revoke_and_early_drop_stop_each_task_once() {
        let batch = read_batch();
        let mut store = ObjectStore::<_, 1>::new(Memory::default());
        let mut launch = Launch::new(&mut store, &batch, DEMO, 7).unwrap();
        let args = launch.arguments();
        let mut session = Session::with_arguments(&batch, &mut launch, 2, args).unwrap();
        session.dispatch().unwrap();
        session.dispatch().unwrap();
        assert!(session.cancel(0));
        assert!(!session.cancel(0));
        assert!(session.revoke());
        drop(session);
        assert!(!launch.revoke());
        assert_eq!(launch.stops(), [1, 1, 1, 0]);
        assert_eq!(launch.stats().objects + launch.stats().payload_frames, 0);
    }
    #[test]
    fn restart_rejects_old_argument_and_foreign_handle_while_new_reader_runs() {
        let batch = read_batch();
        let mut store = ObjectStore::<_, 1>::new(Memory::default());
        let old;
        {
            let mut launch = Launch::new(&mut store, &batch, DEMO, 7).unwrap();
            old = launch.arguments()[0];
            assert!(launch.revoke());
        }
        {
            let mut launch = Launch::new(&mut store, &batch, DEMO, 9).unwrap();
            let mut args = launch.arguments();
            assert_ne!(args[0], old);
            args[0] = old;
            args[2] = args[1];
            let mut session = Session::with_arguments(&batch, &mut launch, 2, args).unwrap();
            for (task, result) in [
                (0, Err(Fault::Authority)),
                (1, Ok(Slice::Event(Outcome::Yield(9)))),
                (2, Err(Fault::Authority)),
                (1, Ok(Slice::Event(Outcome::Halt(9)))),
            ] {
                let event = session.dispatch().unwrap();
                assert_eq!(event.task, task);
                assert_eq!(event.result, result);
            }
            drop(session);
            assert_eq!(launch.stops(), [1, 1, 1, 0]);
            assert_eq!(launch.stats().capabilities, 1);
        }
        assert_eq!(store.stats().payload_frames, 0);
    }
}
