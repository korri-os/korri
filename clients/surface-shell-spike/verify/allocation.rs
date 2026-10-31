//! Count every QuickJS backing allocation, including transient buffers.
//! Delegate allocation and layout to rquickjs's own RustAllocator.
use rquickjs::allocator::{Allocator, RustAllocator};
use std::{cell::Cell, rc::Rc};

pub struct Meter {
    pub current: Cell<usize>,
    pub peak: Cell<usize>,
    pub limit: usize,
}
pub struct Measured(pub Rc<Meter>);
impl Measured {
    fn permitted(&self, size: usize, old: usize) -> bool {
        // RustAllocator rounds payloads to eight bytes. Its private header is
        // excluded from this backing-payload metric, consistently on both CPUs.
        size.checked_add(7)
            .map(|n| n & !7)
            .and_then(|n| (self.0.current.get() - old).checked_add(n))
            .is_some_and(|n| n <= self.0.limit)
    }
    unsafe fn record(&self, ptr: *mut u8, old: usize) {
        if !ptr.is_null() {
            let current = self.0.current.get() - old + RustAllocator::usable_size(ptr);
            self.0.current.set(current);
            self.0.peak.set(self.0.peak.get().max(current));
        }
    }
}
// SAFETY: all pointers and usable sizes come from RustAllocator. Failed
// reallocations preserve the previous pointer and accounting. Null and zero
// sizes are handled here before delegating to its non-null realloc interface.
unsafe impl Allocator for Measured {
    fn alloc(&mut self, size: usize) -> *mut u8 {
        if !self.permitted(size, 0) {
            return std::ptr::null_mut();
        }
        let ptr = RustAllocator.alloc(size);
        unsafe {
            self.record(ptr, 0);
        }
        ptr
    }
    fn calloc(&mut self, count: usize, size: usize) -> *mut u8 {
        let Some(total) = count.checked_mul(size) else {
            return std::ptr::null_mut();
        };
        if !self.permitted(total, 0) {
            return std::ptr::null_mut();
        }
        let ptr = RustAllocator.calloc(count, size);
        unsafe {
            self.record(ptr, 0);
        }
        ptr
    }
    unsafe fn dealloc(&mut self, ptr: *mut u8) {
        if ptr.is_null() {
            return;
        }
        self.0
            .current
            .set(self.0.current.get() - RustAllocator::usable_size(ptr));
        RustAllocator.dealloc(ptr);
    }
    unsafe fn realloc(&mut self, ptr: *mut u8, size: usize) -> *mut u8 {
        if ptr.is_null() {
            return self.alloc(size);
        }
        if size == 0 {
            self.dealloc(ptr);
            return std::ptr::null_mut();
        }
        let old = RustAllocator::usable_size(ptr);
        if !self.permitted(size, old) {
            return std::ptr::null_mut();
        }
        let ptr = RustAllocator.realloc(ptr, size);
        self.record(ptr, old);
        ptr
    }
    unsafe fn usable_size(ptr: *mut u8) -> usize {
        RustAllocator::usable_size(ptr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retains_peak_after_free_and_rejects_limit() {
        let meter = Rc::new(Meter {
            current: Cell::new(0),
            peak: Cell::new(0),
            limit: 1024,
        });
        let mut allocator = Measured(meter.clone());
        let ptr = allocator.alloc(512);
        assert!(!ptr.is_null());
        assert!(allocator.alloc(1024).is_null());
        unsafe {
            allocator.dealloc(ptr);
        }
        assert_eq!(meter.current.get(), 0);
        assert_eq!(meter.peak.get(), 512);
    }
}
