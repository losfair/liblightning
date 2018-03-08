use stack::Stack;

use std::os::raw;
use std::any::Any;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use promise::Promise;

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

struct MaybeYieldVal {
    val: Option<*const Promise>
}

unsafe impl Send for MaybeYieldVal {}

pub trait CommonCoState {
    fn resume(&mut self) -> Option<&Promise>;
    fn take_stack(&mut self) -> Option<Stack>;
}
/// The state of a coroutine.
///
/// Must not be accessed by the coroutine itself.
pub struct CoState<F: FnOnce(&mut Yieldable) + 'static> {
    _stack: Option<Stack>,
    rsp: usize,
    yield_val: MaybeYieldVal,
    error_val: Option<Box<Any + Send>>,
    running_state: RunningState,
    f: Option<F>
}

/// A coroutine's view of itself.
///
/// Only accessible from inside a coroutine.
pub trait Yieldable {
    fn yield_now(&mut self, val: &Promise);
}

impl<F: FnOnce(&mut Yieldable) + 'static> Yieldable for CoState<F> {
    fn yield_now(&mut self, val: &Promise) {
        unsafe {
            self.yield_val = MaybeYieldVal { val: Some(val as *const Promise) };

            let new_rsp = self.rsp;
            __ll_co_yield_now(&mut self.rsp, new_rsp);
        }
    }
}

impl<F: FnOnce(&mut Yieldable) + 'static> CommonCoState for CoState<F> {
    fn resume(&mut self) -> Option<&Promise> {
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

                    self.yield_val.val.take().map(|v| &*v)
                },
                RunningState::Running => {
                    __ll_co_yield_now(&mut self.rsp, new_rsp);

                    if let Some(e) = self.error_val.take() {
                        resume_unwind(e);
                    }

                    self.yield_val.val.take().map(|v| &*v)
                },
                RunningState::Terminated => None
            }
        }
    }

    fn take_stack(&mut self) -> Option<Stack> {
        // We can only safely take the stack of an already terminated coroutine.
        self.ensure_terminated();
        self._stack.take()
    }
}

impl<F: FnOnce(&mut Yieldable) + 'static> CoState<F> {
    pub fn new(stack: Stack, f: F) -> CoState<F> {
        let rsp: usize = stack.initial_rsp();

        CoState {
            _stack: Some(stack),
            rsp: rsp,
            yield_val: MaybeYieldVal { val: None },
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

    unsafe fn terminate_from_inside(&mut self) -> ! {
        self.running_state = RunningState::Terminated;

        self.yield_val.val = None;

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
}

impl<F: FnOnce(&mut Yieldable) + 'static> Drop for CoState<F> {
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
            c.yield_now(&Promise::new_resolved());
        });

        assert!(co.resume().unwrap().is_resolved());
        assert!(co.resume().is_none());
    }

    #[test]
    fn nested_should_work() {
        let mut co = CoState::new(Stack::new(4096), |c| {
            let mut co = CoState::new(Stack::new(4096), |c| {
                c.yield_now(&Promise::new_resolved());
            });
            co.resume();
            co.resume();
            c.yield_now(&Promise::new_resolved());
        });

        assert!(co.resume().unwrap().is_resolved());
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
            c.yield_now(&Promise::new_resolved());
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
