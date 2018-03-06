use stack::Stack;

use std::cell::{Cell, UnsafeCell};
use std::os::raw;
use std::any::Any;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

pub type StackInitializer = extern "C" fn (user_data: *mut raw::c_void);

extern "C" {
    fn __ll_co_yield_now(rsp_save_target: *mut usize, new_rsp: usize);
    fn __ll_init_co_stack(
        rsp_save_target: *mut usize,
        new_rsp: usize,
        initializer: StackInitializer,
        user_data: *mut raw::c_void
    );
}

#[derive(Eq, PartialEq)]
enum RunningState {
    NotStarted,
    Running,
    Terminated
}

pub enum PromiseState<T: Any + 'static> {
    Pending,
    Resolved(T)
}

pub struct Promise<T: Any + 'static> {
    // unsafe notes:
    // Once the state is set to resolved, it should never
    // be set back to pending.
    state: UnsafeCell<PromiseState<T>>
}

pub trait CommonPromise: 'static {
    fn is_resolved(&self) -> bool;
    fn resolved_value(&self) -> Option<&Any>;
}

impl<T: Any + 'static> CommonPromise for Promise<T> {
    fn is_resolved(&self) -> bool {
        let state = unsafe { &*self.state.get() };
        match *state {
            PromiseState::Pending => false,
            PromiseState::Resolved(_) => true
        }
    }

    fn resolved_value(&self) -> Option<&Any> {
        let state = unsafe { &*self.state.get() };
        match *state {
            PromiseState::Pending => None,
            PromiseState::Resolved(ref v) => Some(v)
        }
    }
}


impl<T: Any + 'static> Promise<T> {
    pub fn new() -> Promise<T> {
        Promise {
            state: UnsafeCell::new(PromiseState::Pending)
        }
    }

    pub fn new_resolved(value: T) -> Promise<T> {
        Promise {
            state: UnsafeCell::new(PromiseState::Resolved(value))
        }
    }

    pub fn resolved_value(&self) -> Option<&T> {
        let state = unsafe { &*self.state.get() };
        match *state {
            PromiseState::Pending => None,
            PromiseState::Resolved(ref v) => Some(v)
        }
    }

    pub fn resolve(&self, value: T) {
        unsafe {
            let state = &mut *self.state.get();

            // We should never re-set the value of a resolved promise as this may
            // invalidate all references to the inner value.
            if let PromiseState::Resolved(_) = *state {
                panic!("Attempting to resolve an already resolved promise");
            }

            *self.state.get() = PromiseState::Resolved(value);
        }
    }
}

macro_rules! impl_instant_promise {
    ($t:ty) => {
        impl From<$t> for Promise<$t> {
            fn from(other: $t) -> Promise<$t> {
                Promise::new_resolved(other)
            }
        }
    }
}

impl<T> From<Cell<T>> for Promise<Cell<T>> where T: Any + 'static {
    fn from(other: Cell<T>) -> Promise<Cell<T>> {
        Promise::new_resolved(other)
    }
}

impl_instant_promise!(bool);
impl_instant_promise!(i8);
impl_instant_promise!(i16);
impl_instant_promise!(i32);
impl_instant_promise!(i64);
impl_instant_promise!(isize);
impl_instant_promise!(u8);
impl_instant_promise!(u16);
impl_instant_promise!(u32);
impl_instant_promise!(u64);
impl_instant_promise!(usize);
impl_instant_promise!(String);

/// The state of a coroutine.
///
/// Must not be accessed by the coroutine itself.
pub struct CoState<F: FnOnce(&mut Yieldable)> {
    _stack: Option<Stack>,
    rsp: usize,
    yield_val: Option<*const CommonPromise>,
    error_val: Option<Box<Any + Send>>,
    running_state: RunningState,
    f: Option<F>
}

/// A coroutine's view of itself.
///
/// Only accessible from inside a coroutine.
pub trait Yieldable {
    fn yield_now(&mut self, val: &CommonPromise);
}

impl<F: FnOnce(&mut Yieldable)> Yieldable for CoState<F> {
    fn yield_now(&mut self, val: &CommonPromise) {
        unsafe {
            self.yield_val = Some(val as *const CommonPromise);

            let new_rsp = self.rsp;
            __ll_co_yield_now(&mut self.rsp, new_rsp);
        }
    }
}

impl<F: FnOnce(&mut Yieldable)> CoState<F> {
    pub fn new(stack: Stack, f: F) -> CoState<F> {
        let rsp: usize = stack.initial_rsp();

        CoState {
            _stack: Some(stack),
            rsp: rsp,
            yield_val: None,
            error_val: None,
            running_state: RunningState::NotStarted,
            f: Some(f)
        }
    }

    extern "C" fn co_initializer(user_data: *mut raw::c_void) {
        let this: &mut Self = unsafe { &mut *(user_data as *mut Self) };
        {
            let f = this.f.take().unwrap();
            if let Err(e) = catch_unwind(AssertUnwindSafe(|| f(this))) {
                this.error_val = Some(e);
            }
        }

        // No droppable objects should remain at this point.
        // Otherwise there will be a resource leak.
        unsafe {
            this.terminate_from_inside();
        }
    }

    pub fn resume<'a>(&'a mut self) -> Option<&'a CommonPromise> {
        unsafe {
            let new_rsp = self.rsp;

            match self.running_state {
                RunningState::NotStarted => {
                    self.running_state = RunningState::Running;
                    let rsp = &mut self.rsp as *mut usize;
                    let self_raw = self as *mut Self as *mut raw::c_void;
                    __ll_init_co_stack(rsp, new_rsp, Self::co_initializer, self_raw);

                    if let Some(e) = self.error_val.take() {
                        resume_unwind(e);
                    }

                    self.yield_val.take().map(|v| &*v)
                },
                RunningState::Running => {
                    __ll_co_yield_now(&mut self.rsp, new_rsp);

                    if let Some(e) = self.error_val.take() {
                        resume_unwind(e);
                    }

                    self.yield_val.take().map(|v| &*v)
                },
                RunningState::Terminated => None
            }
        }
    }

    unsafe fn terminate_from_inside(&mut self) -> ! {
        self.running_state = RunningState::Terminated;

        self.yield_val = None;

        let new_rsp = self.rsp;
        __ll_co_yield_now(&mut self.rsp, new_rsp);

        eprintln!("Coroutine termination failed");
        ::std::process::abort();
    }

    fn ensure_terminated(&self) {
        if self.running_state != RunningState::Terminated {
            panic!("The current coroutine is required to be terminated at this point");
        }
    }

    pub fn take_stack(&mut self) -> Option<Stack> {
        // We can only safely take the stack of an already terminated coroutine.
        self.ensure_terminated();
        self._stack.take()
    }
}

impl<F: FnOnce(&mut Yieldable)> Drop for CoState<F> {
    fn drop(&mut self) {
        self.ensure_terminated();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn yield_should_work() {
        let mut co = CoState::new(Stack::new(4096), |c| {
            c.yield_now(&Promise::from(42i32));
        });

        assert!(*co.resume().unwrap().resolved_value().unwrap().downcast_ref::<i32>().unwrap() == 42);
        assert!(co.resume().is_none());
    }

    #[test]
    fn nested_should_work() {
        let mut co = CoState::new(Stack::new(4096), |c| {
            let mut co = CoState::new(Stack::new(4096), |c| {
                c.yield_now(&Promise::from(42i32));
            });
            let v = *co.resume().unwrap().resolved_value().unwrap().downcast_ref::<i32>().unwrap() + 1;
            co.resume();
            c.yield_now(&Promise::from(v));
        });

        assert!(*co.resume().unwrap().resolved_value().unwrap().downcast_ref::<i32>().unwrap() == 43);
        assert!(co.resume().is_none());
    }

    #[test]
    fn panics_should_propagate() {
        // Use a larger stack size here to make backtrace work
        let mut co = CoState::new(Stack::new(16384), |_| {
            panic!("Test panic");
        });
        let e = catch_unwind(AssertUnwindSafe(|| {
            co.resume();
        })).err().unwrap();
        let v: &&'static str = e.downcast_ref().unwrap();
        assert_eq!(*v, "Test panic");
    }

    #[test]
    fn instant_termination_should_work() {
        let mut co = CoState::new(Stack::new(4096), |_| {});
        assert!(co.resume().is_none());
    }

    #[test]
    fn resume_terminated_should_return_none() {
        let mut co = CoState::new(Stack::new(4096), |_| {});
        assert!(co.resume().is_none());
        assert!(co.resume().is_none());
    }

    #[test]
    fn taking_stack_before_termination_should_panic() {
        let mut co = CoState::new(Stack::new(4096), |c| {
            let v: bool = false;
            c.yield_now(&Promise::from(v));
        });
        assert!(co.resume().is_some());

        if let Ok(_) = catch_unwind(AssertUnwindSafe(|| {
            co.take_stack();
        })) {
            panic!("Taking stack of a running coroutine does not panic");
        }

        assert!(co.resume().is_none());
    }

    #[test]
    fn taking_stack_should_work() {
        let mut co = CoState::new(Stack::new(4096), |_| {});
        assert!(co.resume().is_none());

        assert!(co.take_stack().is_some());
        assert!(co.take_stack().is_none());
    }

    // The correct behavior for these two tests is to segfault with
    // a bad permissions error.
    /*#[test]
    fn stack_overflow() {
        fn inner(i: i32) -> i32 {
            if i == 42 {
                inner(42)
            } else {
                0
            }
        }

        let mut co = CoState::new(4096, |_| {
            inner(42);
        });
        co.resume();
    }

    #[test]
    fn really_big_stack_overflow() {
        fn inner(v: i32) -> i32 {
            let mut arr: [i32; 8192] = [0; 8192];
            arr[0] = v;
            for i in 1..8192 {
                arr[i] = arr[i - 1] + arr[8192 - i - 1] + 1;
            }
            arr[1000]
        }
        let mut co = CoState::new(4096, |_| {
            inner(42);
        });
        co.resume();
    }*/
}
