use core::fmt;
use core::mem::ManuallyDrop;
use core::ops::Deref;
use core::ops::DerefMut;
use core::pin::Pin;

use cortex_m::interrupt::CriticalSection;

/// A "lock" based on critical sections
///
/// # Safety
///
/// * The lock is only safe on single-core systems.
/// * The lock can be cloned only because all access to the data goes
///   through critical sections which are guaranteed to execute exclusively.
pub struct Lock<T> {
    inner: Inner<T>,
}

/// One instance of the Lock is going to be the _actual_ data, other instances are
/// just pointers to the actual, which is guaranteed to stay put by virtue of Pin.
enum Inner<T> {
    Instance(Pin<ManuallyDrop<T>>),
    Pointer(*mut T),
}

impl<T> Lock<T> {
    pub fn as_ptr(&self) -> *mut T {
        match &self.inner {
            Inner::Instance(i) => &*i.as_ref() as *const T as *mut T,
            Inner::Pointer(p) => *p,
        }
    }
}

/// This is the whole purpose of this implementation. To allow cloning of the lock without
/// cloning the actual locked in data.
impl<T> Clone for Lock<T> {
    fn clone(&self) -> Self {
        Lock {
            // Any clone is just a pointer to the original.
            inner: Inner::Pointer(self.as_ptr()),
        }
    }
}

/// Locks cannot be dropped. They must life for the entire lifetime of the program.
impl<T> Drop for Lock<T> {
    fn drop(&mut self) {
        panic!("Lock instances are not allowed to drop");
    }
}

pub struct LockGuard<'a, T> {
    data: &'a mut T,
}

impl<T: Unpin> Lock<T> {
    /// Create a new instance of a lock.
    pub fn new(t: T) -> Self {
        Lock {
            inner: Inner::Instance(Pin::new(ManuallyDrop::new(t))),
        }
    }
}

impl<T> Lock<T> {
    /// Get the data for the duration of the critical section.
    pub fn get<'a, 'c>(&'a self, _cs: &'c CriticalSection) -> LockGuard<'c, T>
    where
        'c: 'a,
    {
        // If we are in a single-core environment, and we are running in a
        // a critical section, we know there will be no other mutable references
        // to this data. That's why it's safe to create this mutable reference
        // for the lifetime of the critical section.
        let data = unsafe { &mut *self.as_ptr() };
        LockGuard { data }
    }

    /// Read the value without obtaining the lock.
    pub fn read(&self) -> &T {
        unsafe { &*self.as_ptr() }
    }
}

impl<'a, T> Deref for LockGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<'a, T> DerefMut for LockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl<T: fmt::Debug> fmt::Debug for LockGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: fmt::Display> fmt::Display for LockGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}
