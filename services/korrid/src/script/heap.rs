//! The heap budget of one interpreter.
//!
//! QuickJS reports a refused allocation as an exception, but not on every
//! path: quickjs-ng 0.16.2 can drop a queued promise job when its allocation
//! fails, so queued work that exhausts the heap can end with no job, no
//! rejection and no error. This allocator enforces the budget itself and
//! records every refusal, so a call that reached the limit always fails.

use std::{cell::Cell, rc::Rc};

use rquickjs::allocator::Allocator;

pub(super) const HEAP_BYTES: usize = 16 * 1024 * 1024;

/// Whether the interpreter refused an allocation at the budget.
#[derive(Clone, Default)]
pub(super) struct Exhausted(Rc<Cell<bool>>);

impl Exhausted {
    pub(super) fn get(&self) -> bool {
        self.0.get()
    }
}

/// Counts host allocations, including QuickJS's partly used small-block
/// arenas, against `HEAP_BYTES`.
pub(super) struct Budget {
    used: usize,
    exhausted: Exhausted,
}

impl Budget {
    pub(super) fn new() -> (Self, Exhausted) {
        let exhausted = Exhausted::default();
        (
            Self {
                used: 0,
                exhausted: exhausted.clone(),
            },
            exhausted,
        )
    }

    fn admit(&mut self, released: usize, requested: usize) -> bool {
        let admitted = self.used - released + requested <= HEAP_BYTES;
        if !admitted {
            self.exhausted.0.set(true);
        }
        admitted
    }

    fn record(&mut self, released: usize, ptr: *mut u8) -> *mut u8 {
        if !ptr.is_null() {
            // SAFETY: ptr was just returned by the libc allocator.
            self.used = self.used - released + unsafe { libc::malloc_usable_size(ptr.cast()) };
        }
        ptr
    }
}

// SAFETY: every pointer comes from libc malloc, calloc or realloc, which
// align to at least `usize`; usable_size reports libc's usable size.
unsafe impl Allocator for Budget {
    fn alloc(&mut self, size: usize) -> *mut u8 {
        if !self.admit(0, size) {
            return std::ptr::null_mut();
        }
        // SAFETY: plain libc allocation.
        let ptr = unsafe { libc::malloc(size) }.cast();
        self.record(0, ptr)
    }

    fn calloc(&mut self, count: usize, size: usize) -> *mut u8 {
        let Some(total) = count.checked_mul(size) else {
            return std::ptr::null_mut();
        };
        if !self.admit(0, total) {
            return std::ptr::null_mut();
        }
        // SAFETY: plain libc allocation.
        let ptr = unsafe { libc::calloc(count, size) }.cast();
        self.record(0, ptr)
    }

    unsafe fn dealloc(&mut self, ptr: *mut u8) {
        self.used -= libc::malloc_usable_size(ptr.cast());
        libc::free(ptr.cast());
    }

    unsafe fn realloc(&mut self, ptr: *mut u8, new_size: usize) -> *mut u8 {
        if ptr.is_null() {
            return self.alloc(new_size);
        }
        let old = libc::malloc_usable_size(ptr.cast());
        if new_size == 0 {
            self.dealloc(ptr);
            return std::ptr::null_mut();
        }
        if !self.admit(old, new_size) {
            return std::ptr::null_mut();
        }
        let moved = libc::realloc(ptr.cast(), new_size).cast();
        // A failed realloc keeps the old block and its count.
        self.record(old, moved)
    }

    unsafe fn usable_size(ptr: *mut u8) -> usize {
        libc::malloc_usable_size(ptr.cast())
    }
}
