//! The cell a runtime's `ReactiveState` lives in.
//!
//! A `RefCell` whose borrows, in debug builds, remember where they were
//! taken. A borrow that fails because one is still held -- Lua re-entering
//! the engine while a caller kept the state borrowed across the call --
//! names the borrow it ran into, not only its own site.

use std::cell::{BorrowError, BorrowMutError, Ref, RefCell, RefMut};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
#[cfg(debug_assertions)]
use std::panic::Location;

pub(crate) struct StateCell<T> {
    value: RefCell<T>,
    #[cfg(debug_assertions)]
    held: Held,
}

/// The borrows alive now: an id, where it was taken, whether mutably.
#[cfg(debug_assertions)]
#[derive(Default)]
struct Held {
    sites: RefCell<Vec<(u64, &'static Location<'static>, bool)>>,
    next: std::cell::Cell<u64>,
}

/// Forgets its borrow's site when the borrow ends.
pub struct Mark<'a> {
    #[cfg(debug_assertions)]
    held: &'a Held,
    #[cfg(debug_assertions)]
    id: u64,
    _cell: PhantomData<&'a ()>,
}

impl Drop for Mark<'_> {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        self.held
            .sites
            .borrow_mut()
            .retain(|(id, _, _)| *id != self.id);
    }
}

/// A shared borrow of a runtime's state, or of a part of it.
pub struct StateRef<'a, T: ?Sized> {
    value: Ref<'a, T>,
    _mark: Mark<'a>,
}

/// A mutable borrow of a runtime's state, or of a part of it.
pub struct StateRefMut<'a, T: ?Sized> {
    value: RefMut<'a, T>,
    _mark: Mark<'a>,
}

impl<T> StateCell<T> {
    pub(crate) fn new(value: T) -> Self {
        Self {
            value: RefCell::new(value),
            #[cfg(debug_assertions)]
            held: Held::default(),
        }
    }

    #[track_caller]
    pub(crate) fn borrow(&self) -> StateRef<'_, T> {
        match self.try_borrow() {
            Ok(value) => value,
            Err(_) => self.conflict("borrow"),
        }
    }

    #[track_caller]
    pub(crate) fn borrow_mut(&self) -> StateRefMut<'_, T> {
        match self.try_borrow_mut() {
            Ok(value) => value,
            Err(_) => self.conflict("borrow_mut"),
        }
    }

    #[track_caller]
    pub(crate) fn try_borrow(&self) -> Result<StateRef<'_, T>, BorrowError> {
        let value = self.value.try_borrow()?;
        Ok(StateRef {
            value,
            _mark: self.mark(false),
        })
    }

    #[track_caller]
    pub(crate) fn try_borrow_mut(&self) -> Result<StateRefMut<'_, T>, BorrowMutError> {
        let value = self.value.try_borrow_mut()?;
        Ok(StateRefMut {
            value,
            _mark: self.mark(true),
        })
    }

    #[track_caller]
    fn mark(&self, _mutable: bool) -> Mark<'_> {
        #[cfg(debug_assertions)]
        {
            let id = self.held.next.get();
            self.held.next.set(id.wrapping_add(1));
            self.held
                .sites
                .borrow_mut()
                .push((id, Location::caller(), _mutable));
            Mark {
                held: &self.held,
                id,
                _cell: PhantomData,
            }
        }
        #[cfg(not(debug_assertions))]
        Mark { _cell: PhantomData }
    }

    /// Panics for a borrow that ran into one still held, naming that one.
    #[track_caller]
    fn conflict(&self, what: &str) -> ! {
        #[cfg(debug_assertions)]
        {
            let held = self
                .held
                .sites
                .borrow()
                .iter()
                .map(|(_, site, mutable)| {
                    format!(
                        "{} at {site}",
                        if *mutable { "borrow_mut" } else { "borrow" }
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            panic!(
                "ReactiveState {what} at {} re-entered while still held: {held}",
                std::panic::Location::caller()
            );
        }
        #[cfg(not(debug_assertions))]
        panic!("ReactiveState {what} re-entered while still held");
    }
}

impl<'a, T: ?Sized> StateRef<'a, T> {
    /// Narrows the borrow to a part of the value, keeping its site.
    pub fn map<U: ?Sized>(this: Self, f: impl FnOnce(&T) -> &U) -> StateRef<'a, U> {
        StateRef {
            value: Ref::map(this.value, f),
            _mark: this._mark,
        }
    }
}

impl<'a, T: ?Sized> StateRefMut<'a, T> {
    /// Narrows the borrow to a part of the value, keeping its site.
    pub fn map<U: ?Sized>(this: Self, f: impl FnOnce(&mut T) -> &mut U) -> StateRefMut<'a, U> {
        StateRefMut {
            value: RefMut::map(this.value, f),
            _mark: this._mark,
        }
    }
}

impl<T: ?Sized> Deref for StateRef<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T: ?Sized> Deref for StateRefMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T: ?Sized> DerefMut for StateRefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(debug_assertions)]
    fn a_reentrant_borrow_names_the_one_it_ran_into() {
        let cell = StateCell::new(0);
        let outer = cell.borrow_mut();
        let line = line!() - 1;
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _inner = cell.borrow();
        }))
        .expect_err("the inner borrow fails");
        let message = failed.downcast_ref::<String>().cloned().unwrap_or_default();
        assert!(
            message.contains(&format!("borrow_mut at {}:{line}", file!())),
            "{message}"
        );
        drop(outer);
        assert!(cell.held.sites.borrow().is_empty());
        let _shared = (cell.borrow(), cell.borrow());
        assert!(cell.try_borrow_mut().is_err());
    }
}
