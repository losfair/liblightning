use std::time::Duration;
use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe, resume_unwind};
use std::any::Any;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::cell::{Cell, RefCell};
use co::{CommonCoState, CoState, Yieldable, SendableCoState};
use stack_pool::{StackPool, StackPoolConfig};
use promise::{Promise, PromiseBegin, NotifyHandle};
use invoke_box::OnceInvokeBox;

pub struct Scheduler {
    state: SharedSchedState
}

#[derive(Clone)]
pub struct SharedSchedState {
    inner: Rc<RefCell<SharedSchedStateImpl>>
}

pub struct SharedSchedStateImpl {
    free_stacks: StackPool,
    termination_requested: bool,
    running_cos: VecDeque<Box<CommonCoState>>,
    sync_state: SyncSchedState
}

#[derive(Clone)]
pub struct SyncSchedState {
    inner: Arc<Mutex<SyncSchedStateImpl>>
}

pub struct SyncSchedStateImpl {
    pending_cos: Vec<Box<CommonCoState>>
}

pub struct SchedulerConfig {
    pub stack_pool: StackPool
}

pub struct ValuePromise<T: 'static> {
    pub notify: Promise,
    pub value: Rc<Cell<Option<T>>>
}

impl<T: 'static> ValuePromise<T> {
    pub fn take_value(&self) -> Option<T> {
        self.value.replace(None)
    }
}

unsafe impl Send for SyncSchedStateImpl {}

impl SyncSchedState {
    // This is unsafe because we cannot check whether the coroutine originally
    // belongs to the sched state.
    pub(crate) unsafe fn add_coroutine(&self, co: SendableCoState) {
        self.inner.lock().unwrap().pending_cos.push(co.unwrap());
    }
}

impl SharedSchedState {
    pub fn get_sync(&self) -> SyncSchedState {
        self.inner.borrow().sync_state.clone()
    }

    pub(crate) fn push_coroutine_raw(&self, co: Box<CommonCoState>) {
        self.inner.borrow_mut().running_cos.push_back(co);
    }

    pub fn start_coroutine<F: FnOnce(&mut Yieldable) + 'static>(&self, f: F) {
        let mut this = self.inner.borrow_mut();
        let stack = this.free_stacks.get();
        this.running_cos.push_back(Box::new(CoState::new(
            stack,
            f
        )));
    }

    pub fn prepare_coroutine<R: 'static, F: FnOnce(&mut Yieldable) -> R + 'static>(&self, f: F) -> ValuePromise<Result<R, Box<Any + Send>>> {
        let value: Rc<Cell<Option<Result<R, Box<Any + Send>>>>> = Rc::new(Cell::new(None));
        let value2 = value.clone();

        let this = self.clone();
    
        let vp: ValuePromise<Result<R, Box<Any + Send>>> = ValuePromise {
            notify: Promise::new(move |cb| {
                this.start_coroutine(move |c| {
                    value2.set(Some(catch_unwind(AssertUnwindSafe(move || f(c)))));
                    cb.notify();
                })
            }),
            value: value
        };
        vp
    }

    pub fn terminate(&self) {
        self.inner.borrow_mut().termination_requested = true;
    }
}

impl Scheduler {
    pub fn new(config: SchedulerConfig) -> Scheduler {
        Scheduler {
            state: SharedSchedState {
                inner: Rc::new(RefCell::new(SharedSchedStateImpl {
                    free_stacks: config.stack_pool,
                    termination_requested: false,
                    running_cos: VecDeque::new(),
                    sync_state: SyncSchedState {
                        inner: Arc::new(Mutex::new(SyncSchedStateImpl {
                            pending_cos: Vec::new()
                        }))
                    }
                }))
            }
        }
    }

    pub fn new_default() -> Scheduler {
        Self::new(SchedulerConfig {
            stack_pool: StackPool::new(StackPoolConfig::default())
        })
    }

    pub fn get_state(&self) -> SharedSchedState {
        self.state.clone()
    }

    pub fn run_value_promise_to_end<T: 'static>(&mut self, vp: ValuePromise<T>) -> T {
        let vp = Rc::new(vp);

        let vp2 = vp.clone();
        let state = self.state.clone();
    
        self.state.start_coroutine(move |c| {
            c.yield_now(&vp2.notify);
            state.terminate();
        });
        self.run();

        vp.take_value().unwrap()
    }

    pub fn run(&mut self) {
        let mut sleep_micros: u64 = 0;
        let mut run_count: usize = 0;

        loop {
            run_count += 1;
            if run_count == 60 {
                run_count = 0;
            }

            if run_count == 0 {
                let mut state = self.state.inner.borrow_mut();
                let pending = ::std::mem::replace(
                    &mut state.sync_state.inner.lock().unwrap().pending_cos,
                    Vec::new()
                );
                state.running_cos.extend(pending.into_iter());
            }

            let termination_requested;

            let co = {
                let mut state = self.state.inner.borrow_mut();

                // Scheduler should not be terminated until all coroutines has ended.
                // Defer termination check here.
                termination_requested = state.termination_requested;

                state.running_cos.pop_front()
            };

            let mut co = if let Some(co) = co {
                sleep_micros = 0;
                co
            } else {
                if termination_requested {
                    self.state.inner.borrow_mut().termination_requested = false;
                    return;
                }

                if sleep_micros < 100 {
                    sleep_micros += 1;
                } else {
                    let millis = sleep_micros / 1000;
                    if millis > 0 {
                        ::std::thread::sleep(Duration::from_millis(millis));
                    }
                    if sleep_micros < 5000 { // 5ms
                        sleep_micros *= 2; // exponential
                    } else if sleep_micros < 50000 { // 50ms
                        sleep_micros += 100; // linear
                    }
                }
                continue;
            };

            // Workaround for borrowck issues. Should be removed once NLL lands in stable Rust.
            enum CurrentPromiseState {
                Started,
                Async(PromiseBegin),
                Terminated
            }

            // Currently all code below has to be in a catch_unwind block (NLL?)
            if let Err(_) = catch_unwind(AssertUnwindSafe(|| {
                let ps = {
                    let ret = co.resume();

                    // Promise.
                    if let Some(p) = ret {
                        // This promise contains an instant value.
                        if p.is_started() {
                            CurrentPromiseState::Started
                        } else { // Some async operations required.
                            CurrentPromiseState::Async(p.build_begin())
                        }
                    } else {
                        // The current coroutine is terminated
                        CurrentPromiseState::Terminated
                    }
                };
                match ps {
                    CurrentPromiseState::Started => {
                        self.state.inner.borrow_mut().running_cos.push_back(co);
                    },
                    CurrentPromiseState::Async(begin) => {
                        let state = self.state.clone();
                        begin.run(NotifyHandle::new(state, co));
                    },
                    CurrentPromiseState::Terminated => {
                        let stack = co.take_stack().unwrap();
                        self.state.inner.borrow_mut().free_stacks.put(stack);
                    }
                }
            })) {
                eprintln!("Error in coroutine");
            }
        }
    }
}

#[cfg(test)]
#[allow(unused_imports)]
mod tests {
    use super::*;
    use std::panic::{catch_unwind, AssertUnwindSafe, resume_unwind};
    use std::cell::Cell;
    use std::rc::Rc;
    use std::any::Any;

    #[test]
    fn coroutines_should_be_scheduled() {
        let mut sched = Scheduler::new_default();
        let state = sched.state.clone();

        let vp = sched.state.prepare_coroutine(move |c| {
            let value: Rc<Cell<i32>> = Rc::new(Cell::new(0));
            let value2 = value.clone();
            let p = Promise::new(move |cb| {
                value2.set(42);
                cb.notify();
            });
            assert_eq!(value.get(), 0);
            c.yield_now(&p);
            assert_eq!(value.get(), 42);

            let p = state.prepare_coroutine(move |_| {
                panic!("Test panic");
            });
            c.yield_now(&p.notify);
            assert_eq!(*p.value.replace(None).unwrap().err().unwrap().downcast_ref::<&'static str>().unwrap(), "Test panic");
        });
        let ret = sched.run_value_promise_to_end(vp);

        match ret {
            Ok(_) => {},
            Err(e) => resume_unwind(e)
        }
    }
}
