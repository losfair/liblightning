use std::time::Duration;
use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Sender, Receiver};
use std::any::Any;
use std::rc::Rc;
use std::cell::{Cell, RefCell};
use co::{CommonCoState, CoState, Yieldable};
use stack_pool::{StackPool, StackPoolConfig};
use promise::{Promise, PromiseBegin};
use invoke_box::OnceInvokeBox;

pub struct Scheduler {
    pub state: SharedSchedState,
    task_feed: Receiver<Box<CommonCoState>>
}

#[derive(Clone)]
pub struct SharedSchedState {
    inner: Arc<Mutex<SharedSchedStateImpl>>
}

pub struct SharedSchedStateImpl {
    free_stacks: StackPool,
    termination_requested: bool,
    running_cos: VecDeque<Box<CommonCoState>>,
    task_sender: Sender<Box<CommonCoState>>
}

pub struct SchedulerConfig {
    pub stack_pool: StackPool
}

pub struct ValuePromise<T: 'static> {
    notify: Promise,
    value: Rc<Cell<Option<T>>>
}

impl<T: 'static> ValuePromise<T> {
    pub fn take_value(&self) -> Option<T> {
        self.value.replace(None)
    }

    pub fn is_resolved(&self) -> bool {
        self.notify.is_resolved()
    }

    pub fn resolve(&self, v: T) {
        self.value.replace(Some(v));
        self.notify.resolve();
    }
}

impl SharedSchedState {
    pub fn start_coroutine_raw<F: FnOnce(&mut Yieldable) + 'static>(&self, f: F) {
        let mut this = self.inner.lock().unwrap();
        let stack = this.free_stacks.get();
        this.running_cos.push_back(Box::new(CoState::new(
            stack,
            f
        )));
    }

    pub fn run_coroutine<R: 'static, F: FnOnce(&mut Yieldable) -> R + 'static>(&self, f: F) -> ValuePromise<Result<R, Box<Any + Send>>> {
        let value: Rc<Cell<Option<Result<R, Box<Any + Send>>>>> = Rc::new(Cell::new(None));
        let value2 = value.clone();

        let this = self.clone();
    
        let vp: ValuePromise<Result<R, Box<Any + Send>>> = ValuePromise {
            notify: Promise::new(move |cb| {
                this.start_coroutine_raw(move |c| {
                    value2.set(Some(catch_unwind(AssertUnwindSafe(move || f(c)))));
                    cb.call(());
                })
            }),
            value: value
        };
        vp
    }

    pub fn terminate(&self) {
        self.inner.lock().unwrap().termination_requested = true;
    }
}

impl Scheduler {
    pub fn new(config: SchedulerConfig) -> Scheduler {
        let (tx, rx) = ::std::sync::mpsc::channel();

        Scheduler {
            state: SharedSchedState {
                inner: Arc::new(Mutex::new(SharedSchedStateImpl {
                    free_stacks: config.stack_pool,
                    termination_requested: false,
                    running_cos: VecDeque::new(),
                    task_sender: tx
                }))
            },
            task_feed: rx
        }
    }

    pub fn start_promise(&mut self, p: &Promise) {
        let begin = p.build_begin();
        let state = self.state.clone();

        begin.run(OnceInvokeBox::new(move |()| {
        }));
    }

    pub fn run(&mut self) {
        loop {
            let termination_requested;

            let co = {
                let mut state = self.state.inner.lock().unwrap();

                // Scheduler should not be terminated until all coroutines has ended.
                // Defer termination check here.
                termination_requested = state.termination_requested;

                // This should not block.
                while let Ok(v) = self.task_feed.try_recv() {
                    state.running_cos.push_back(v);
                }

                state.running_cos.pop_front()
            };

            let mut co = if let Some(co) = co {
                co
            } else {
                if termination_requested {
                    return;
                }
                // TODO: Latency fix
                ::std::thread::sleep(Duration::from_millis(10));
                continue;
            };

            // Workaround for borrowck issues. Should be removed once NLL lands in stable Rust.
            enum CurrentPromiseState {
                Resolved,
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
                        if p.is_resolved() {
                            CurrentPromiseState::Resolved
                        } else { // Some async operations required.
                            CurrentPromiseState::Async(p.build_begin())
                        }
                    } else {
                        // The current coroutine is terminated
                        CurrentPromiseState::Terminated
                    }
                };
                match ps {
                    CurrentPromiseState::Resolved => {
                        self.state.inner.lock().unwrap().running_cos.push_back(co);
                    },
                    CurrentPromiseState::Async(begin) => {
                        let state = self.state.clone();

                        // NLL required for this to work
                        begin.run(OnceInvokeBox::new(move |()| {
                            state.inner.lock().unwrap().running_cos.push_back(co);
                        }));
                    },
                    CurrentPromiseState::Terminated => {
                        let stack = co.take_stack().unwrap();
                        self.state.inner.lock().unwrap().free_stacks.put(stack);
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
        let mut sched = Scheduler::new(SchedulerConfig {
            stack_pool: StackPool::new(StackPoolConfig::default())
        });
        let state = sched.state.clone();

        let ret: Rc<Cell<Option<Result<(), Box<Any + Send>>>>> = Rc::new(Cell::new(None));
        let ret2 = ret.clone();

        let p = Rc::new(sched.state.run_coroutine(move |c| {
            let value: Rc<Cell<i32>> = Rc::new(Cell::new(0));
            let value2 = value.clone();
            let p = Promise::new(move |cb| {
                value2.set(42);
                cb.call(());
            });
            assert_eq!(value.get(), 0);
            c.yield_now(&p);
            assert_eq!(value.get(), 42);

            let p = state.run_coroutine(move |c| {
                panic!("Test panic");
            });
            c.yield_now(&p.notify);
            assert_eq!(*p.value.replace(None).unwrap().err().unwrap().downcast_ref::<&'static str>().unwrap(), "Test panic");
        }));

        let p2 = p.clone();

        let state = sched.state.clone();
    
        sched.state.start_coroutine_raw(move |c| {
            c.yield_now(&p2.notify);
            state.terminate();
        });
        sched.run();

        let ret = p.take_value().unwrap_or_else(|| {
            panic!("Coroutine did not set its result");
        });
        match ret {
            Ok(_) => {},
            Err(e) => resume_unwind(e)
        }
    }
}
