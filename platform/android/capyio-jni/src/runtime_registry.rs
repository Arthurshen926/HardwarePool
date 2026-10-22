//! Internal authoritative registry guard. Reconciliation runs before release,
//! including ordinary error exits, unwinding and pre-existing poisoning. The
//! production callback only validates borrowed authority and closes atomics.
use std::{
    ops::{Deref, DerefMut},
    sync::{LockResult, Mutex, MutexGuard, PoisonError, TryLockError, TryLockResult},
};

pub(crate) struct ReconciledMutex<T> {
    inner: Mutex<T>,
    reconcile: fn(&T, bool),
}
pub(crate) struct ReconciledGuard<'a, T> {
    inner: MutexGuard<'a, T>,
    reconcile: fn(&T, bool),
    poisoned: bool,
}
impl<T> ReconciledMutex<T> {
    pub(crate) fn new(value: T, reconcile: fn(&T, bool)) -> Self {
        Self {
            inner: Mutex::new(value),
            reconcile,
        }
    }
    pub(crate) fn lock(&self) -> LockResult<ReconciledGuard<'_, T>> {
        match self.inner.lock() {
            Ok(inner) => Ok(ReconciledGuard {
                inner,
                reconcile: self.reconcile,
                poisoned: false,
            }),
            Err(error) => Err(PoisonError::new(ReconciledGuard {
                inner: error.into_inner(),
                reconcile: self.reconcile,
                poisoned: true,
            })),
        }
    }
    pub(crate) fn try_lock(&self) -> TryLockResult<ReconciledGuard<'_, T>> {
        match self.inner.try_lock() {
            Ok(inner) => Ok(ReconciledGuard {
                inner,
                reconcile: self.reconcile,
                poisoned: false,
            }),
            Err(TryLockError::WouldBlock) => Err(TryLockError::WouldBlock),
            Err(TryLockError::Poisoned(error)) => {
                Err(TryLockError::Poisoned(PoisonError::new(ReconciledGuard {
                    inner: error.into_inner(),
                    reconcile: self.reconcile,
                    poisoned: true,
                })))
            }
        }
    }
}
impl<T> Deref for ReconciledGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl<T> DerefMut for ReconciledGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
impl<T> Drop for ReconciledGuard<'_, T> {
    fn drop(&mut self) {
        (self.reconcile)(&self.inner, self.poisoned || std::thread::panicking());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        sync::{Arc, Weak},
    };

    type Events = Arc<Mutex<Vec<(usize, bool, bool)>>>;
    struct State {
        owner: Weak<ReconciledMutex<State>>,
        events: Events,
        revision: usize,
    }
    fn observe(state: &State, failed: bool) {
        let owner = state.owner.upgrade().expect("test owner alive");
        let held = matches!(owner.try_lock(), Err(TryLockError::WouldBlock));
        state
            .events
            .lock()
            .unwrap()
            .push((state.revision, failed, held));
    }
    fn registry() -> (Arc<ReconciledMutex<State>>, Events) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let owner = Arc::new_cyclic(|weak| {
            ReconciledMutex::new(
                State {
                    owner: weak.clone(),
                    events: events.clone(),
                    revision: 0,
                },
                observe,
            )
        });
        (owner, events)
    }

    #[test]
    fn registry_guard_reconciles_before_normal_error_release() {
        let (owner, events) = registry();
        let mutation = || -> Result<(), &'static str> {
            let mut guard = owner.lock().map_err(|_| "poisoned")?;
            guard.revision = 7;
            Err("ordinary mutation failure")
        };
        assert_eq!(mutation(), Err("ordinary mutation failure"));
        assert_eq!(*events.lock().unwrap(), [(7, false, true)]);
        assert!(!owner.inner.is_poisoned());
        let guard = owner.try_lock().expect("error released registry");
        assert_eq!(guard.revision, 7);
        drop(guard);
        assert_eq!(
            *events.lock().unwrap(),
            [(7, false, true), (7, false, true)]
        );
    }

    #[test]
    fn registry_guard_reconciles_during_unwind_preserving_poison() {
        let (owner, events) = registry();
        let failure = catch_unwind(AssertUnwindSafe(|| {
            let mut guard = owner.lock().unwrap();
            guard.revision = 11;
            panic!("synthetic authoritative mutation panic");
        }));
        assert!(failure.is_err());
        assert_eq!(*events.lock().unwrap(), [(11, true, true)]);
        assert!(owner.inner.is_poisoned());
        let error = match owner.lock() {
            Err(error) => error,
            Ok(_) => panic!("poison was lost"),
        };
        let mut guard = error.into_inner();
        assert_eq!(guard.revision, 11);
        guard.revision = 12;
        drop(guard);
        assert!(
            owner.inner.is_poisoned(),
            "reading poison must not clear it"
        );
        assert_eq!(
            *events.lock().unwrap(),
            [(11, true, true), (12, true, true)]
        );
    }

    #[test]
    fn registry_guard_try_lock_preserves_contention_and_poison() {
        let (owner, events) = registry();
        let guard = owner.lock().unwrap();
        assert!(matches!(owner.try_lock(), Err(TryLockError::WouldBlock)));
        assert!(
            events.lock().unwrap().is_empty(),
            "busy never acquired a guard"
        );
        drop(guard);
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _guard = owner.lock().unwrap();
            panic!("synthetic poison");
        }));
        let error = match owner.try_lock() {
            Err(TryLockError::Poisoned(error)) => error,
            _ => panic!("try_lock must preserve poison, not report contention"),
        };
        drop(error);
        assert_eq!(
            *events.lock().unwrap(),
            [(0, false, true), (0, true, true), (0, true, true)]
        );
    }
}
