//! A host-bound RAII execution scope for a complete validated external batch.
use crate::{
    loader::Batch,
    scheduler::{AdmissionError, Dispatch, Scheduler, TaskHost},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionError {
    Empty,
    Admission(AdmissionError),
}

/// Trusted control path, never a bytecode host call. Revocation is synchronous,
/// leaves scheduling/fuel unchanged, and returns false when already revoked.
pub trait RevocableHost: TaskHost {
    fn revoke(&mut self) -> bool;
}

/// The host and code stay borrowed until the session ends. Drop cancels live
/// tasks; stopped tasks are not cleaned twice. The trusted host must implement
/// infallible, non-reentrant stop and own exactly the admitted task tables.
/// This does not run on panic=abort, power loss or deliberate mem::forget.
///
/// Code cannot be cleared during execution:
/// ```compile_fail
/// use shared_first_os::{loader::Batch, scheduler::TaskHost, session::Session};
/// fn invalid<H: TaskHost>(batch: &mut Batch, host: &mut H) {
///     let mut session = Session::new(batch, host, 2).unwrap();
///     batch.clear();
///     session.dispatch();
/// }
/// ```
/// Nor can the authority host be used independently while the session is live:
/// ```compile_fail
/// use shared_first_os::{loader::Batch, scheduler::TaskHost, session::Session};
/// fn invalid<H: TaskHost>(batch: &Batch, host: &mut H) {
///     let mut session = Session::new(batch, host, 2).unwrap();
///     host.stop(0);
///     session.dispatch();
/// }
/// ```
/// Both become available again after Drop:
/// ```no_run
/// use shared_first_os::{loader::Batch, scheduler::TaskHost, session::Session};
/// fn valid<H: TaskHost>(batch: &mut Batch, host: &mut H) {
///     let session = Session::new(batch, host, 2).unwrap();
///     drop(session);
///     batch.clear();
///     let _: &mut H = host; // Caller can borrow the host exclusively again.
/// }
/// ```
#[must_use = "dropping a session cancels its live tasks"]
pub struct Session<'code, 'host, H: TaskHost> {
    scheduler: Scheduler<'code>,
    host: &'host mut H,
}
impl<'code, 'host, H: TaskHost> Session<'code, 'host, H> {
    pub fn new(
        batch: &'code Batch,
        host: &'host mut H,
        quantum: u32,
    ) -> Result<Self, SessionError> {
        Self::with_arguments(batch, host, quantum, [0; crate::scheduler::CAPACITY])
    }
    /// Arguments are supplied by trusted launch policy, never package metadata.
    pub fn with_arguments(
        batch: &'code Batch,
        host: &'host mut H,
        quantum: u32,
        arguments: [u64; crate::scheduler::CAPACITY],
    ) -> Result<Self, SessionError> {
        if batch.count() == 0 {
            return Err(SessionError::Empty);
        }
        // Configuration is checked before adopting any authority. On this error
        // the caller still owns all host resources and is responsible for them.
        let scheduler = Scheduler::new(quantum).map_err(SessionError::Admission)?;
        let mut session = Self { scheduler, host };
        for (index, argument) in arguments.into_iter().enumerate().take(batch.count()) {
            let (program, fuel) = batch.program(index).expect("validated batch index");
            // Batch quotas guarantee success for a fresh scheduler. If admission
            // ever becomes fallible here, Drop also cleans the admitted prefix.
            session
                .scheduler
                .admit_with_argument(program, fuel, argument)
                .map_err(SessionError::Admission)?;
        }
        Ok(session)
    }
    pub fn dispatch(&mut self) -> Option<Dispatch> {
        self.scheduler.dispatch(self.host)
    }
    pub fn cancel(&mut self, task: usize) -> bool {
        self.scheduler.cancel(task, self.host)
    }
    pub fn ready(&self) -> usize {
        self.scheduler.ready()
    }
}
impl<H: TaskHost> Drop for Session<'_, '_, H> {
    fn drop(&mut self) {
        self.scheduler.shutdown(self.host);
    }
}
impl<H: RevocableHost> Session<'_, '_, H> {
    /// Revoke through the same borrowed host without exposing it for replacement.
    pub fn revoke(&mut self) -> bool {
        self.host.revoke()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        loader::MAGIC,
        sandbox::{Fault, Outcome, Slice, instruction},
    };
    struct World {
        stops: [usize; 4],
        calls: usize,
    }
    impl TaskHost for World {
        fn sum(&mut self, _: usize, _: u64, _: u64, _: u64) -> Result<u64, Fault> {
            self.calls += 1;
            Err(Fault::Authority)
        }
        fn stop(&mut self, task: usize) {
            self.stops[task] += 1;
        }
    }
    fn batch(opcode: u8) -> Batch {
        let mut image = [0; 112];
        image[..8].copy_from_slice(&MAGIC);
        image[8] = 1;
        image[10] = 3;
        for index in 0..3 {
            let offset = 16 + 32 * index;
            image[offset] = 16;
            image[offset + 4] = 4;
            image[offset + 16..offset + 32].copy_from_slice(&instruction(
                if index == 0 { opcode } else { 0 },
                0,
                0,
                0,
                if opcode == 2 && index == 0 { 1 } else { 0 },
            ));
        }
        Batch::load(&image).unwrap()
    }
    #[test]
    fn drop_before_start_cancels_all_and_unborrows_code() {
        let mut batch = batch(0);
        let mut host = World {
            stops: [0; 4],
            calls: 0,
        };
        drop(Session::new(&batch, &mut host, 2).unwrap());
        assert_eq!(host.stops, [1, 1, 1, 0]);
        batch.clear();
        assert_eq!(batch.code_bytes(), 0);
    }
    #[test]
    fn early_return_after_fault_cleans_other_tasks_once() {
        let batch = batch(2);
        let mut host = World {
            stops: [0; 4],
            calls: 0,
        };
        fn early(batch: &Batch, host: &mut World) -> Result<(), Fault> {
            let mut session = Session::new(batch, host, 2).unwrap();
            session.dispatch().unwrap().result?;
            Ok(())
        }
        assert_eq!(early(&batch, &mut host), Err(Fault::Authority));
        assert_eq!(host.stops, [1, 1, 1, 0]);
        assert_eq!(host.calls, 1);
    }
    #[test]
    fn normal_exit_cancel_and_drop_do_not_duplicate_cleanup() {
        let batch = batch(0);
        let mut host = World {
            stops: [0; 4],
            calls: 0,
        };
        {
            let mut session = Session::new(&batch, &mut host, 1).unwrap();
            assert!(session.cancel(1));
            assert!(!session.cancel(1));
            assert!(!session.cancel(usize::MAX));
            assert_eq!(
                session.dispatch().unwrap().result,
                Ok(Slice::Event(Outcome::Halt(0)))
            );
            assert_eq!(session.ready(), 1);
            assert_eq!(session.dispatch().unwrap().task, 2);
            assert!(session.dispatch().is_none());
            assert_eq!(session.ready(), 0);
        }
        assert_eq!(host.stops, [1, 1, 1, 0]);
    }
    #[test]
    fn invalid_configuration_does_not_adopt_host() {
        let mut batch = batch(0);
        let mut host = World {
            stops: [0; 4],
            calls: 0,
        };
        assert!(matches!(
            Session::new(&batch, &mut host, 0),
            Err(SessionError::Admission(AdmissionError::Quantum))
        ));
        batch.clear();
        assert!(matches!(
            Session::new(&batch, &mut host, 2),
            Err(SessionError::Empty)
        ));
        assert_eq!(host.stops, [0; 4]);
    }
}
