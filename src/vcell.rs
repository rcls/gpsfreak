#![allow(dead_code)]

use core::cell::UnsafeCell;

/// Interrupt safe volatile cell, has read/write for scalar types.
#[repr(transparent)]
pub struct VCell<T>(UnsafeCell<T>);

impl<T: Sync + const Default> const Default for VCell<T> {
    fn default() -> Self {Self::new(T::default())}
}

/// A basic cell for storage.  Shared access is safe, mutable access is
/// unsafe.
#[repr(transparent)]
#[derive_const(Default)]
pub struct UCell<T>(UnsafeCell<T>);

unsafe impl<T> Sync for VCell<T> {}
unsafe impl<T: Sync> Sync for UCell<T> {}

impl<T: Sync> VCell<T> {
    pub const fn new(v: T) -> Self {Self(UnsafeCell::new(v))}
    pub fn as_ptr(&self) -> *mut T {self.0.get()}
    pub fn as_mut(&mut self) -> &mut T {self.0.get_mut()}
}

impl<T: Sync> UCell<T> {
    pub const fn new(v: T) -> Self {Self(UnsafeCell::new(v))}
    pub fn as_ref(&self) -> &T {unsafe{&*(self.0.get() as *const T)}}
    pub fn as_ptr(&self) -> *mut T {self.0.get()}
    /// We are naughty and use this in interrupts, using barriers.
    pub unsafe fn as_mut(&self) -> &mut T {unsafe{&mut *self.0.get()}}
}

#[inline(always)]
pub fn barrier() {
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
}

#[inline(always)]
pub fn nothing() {
    unsafe {core::arch::asm!("", options(nomem))}
}

pub mod interrupt {
    // We don't use disabling interrupts to transfer ownership, so no need for
    // the enable to be unsafe.
    #[cfg(target_arch = "arm")]
    pub fn enable() {unsafe{cortex_m::interrupt::enable()}}
    #[cfg(target_arch = "arm")]
    pub fn disable() {cortex_m::interrupt::disable()}
    #[cfg(not(target_arch = "arm"))]
    pub fn enable() { }
    #[cfg(not(target_arch = "arm"))]
    pub fn disable() { }
}

impl<T: Sync> core::ops::Deref for UCell<T> {
    type Target = T;
    fn deref(&self) -> &T {self.as_ref()}
}

macro_rules! VCellImpl {
    ($($t:ty),*) => {$(
        impl VCell<$t> {
            #[inline(always)]
            pub fn read(&self) -> $t {
                unsafe {core::ptr::read_volatile(self.as_ptr())}
            }
            #[inline(always)]
            pub fn write(&self, v: $t) {
                unsafe {core::ptr::write_volatile(self.as_ptr(), v)};
            }
        }
    )*};
}

VCellImpl!(bool, i8, u8, i16, u16, i32, u32, isize, usize, char);
