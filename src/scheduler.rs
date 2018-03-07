use std::time::Duration;
use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Sender, Receiver};
use co::{CommonCoState, CoState, Yieldable, Promise, OnceInvokeBox};
use stack_pool::{StackPool, StackPoolConfig};

pub struct Scheduler {
    pub state: Arc<Mutex<SharedSchedState>>,
    task_feed: Receiver<Box<CommonCoState>>
}

pub struct SharedSchedState {
    free_stacks: StackPool,
    termination_requested: bool,
    running_cos: VecDeque<Box<CommonCoState>>,
    task_sender: Sender<Box<CommonCoState>>
}

pub struct SchedulerConfig {
    pub stack_pool: StackPool
}

impl SharedSchedState {
    pub fn run_coroutine<F: FnOnce(&mut Yieldable) + Send + 'static>(&mut self, f: F) {
        let co = CoState::new(self.free_stacks.get(), f);
        self.running_cos.push_back(Box::new(co));
    }
}

impl Scheduler {
    pub fn new(config: SchedulerConfig) -> Scheduler {
        let (tx, rx) = ::std::sync::mpsc::channel();

        Scheduler {
            state: Arc::new(Mutex::new(SharedSchedState {
                free_stacks: config.stack_pool,
                termination_requested: false,
                running_cos: VecDeque::new(),
                task_sender: tx
            })),
            task_feed: rx
        }
    }

    pub fn run(&mut self) -> ! {
        loop {
            let co = {
                let mut state = self.state.lock().unwrap();

                if state.termination_requested {
                    panic!("Termination requested");
                }

                // This should not block.
                while let Ok(v) = self.task_feed.try_recv() {
                    state.running_cos.push_back(v);
                }

                state.running_cos.pop_front()
            };

            let mut co = if let Some(co) = co {
                co
            } else {
                // TODO: Latency fix
                ::std::thread::sleep(Duration::from_millis(10));
                continue;
            };

            // Currently all code below has to be in a catch_unwind block (NLL?)
            if let Err(_) = catch_unwind(AssertUnwindSafe(|| {
                let ret = co.resume();

                // Promise.
                if let Some(p) = ret {
                    // This promise contains an instant value.
                    if p.is_resolved() {
                        self.state.lock().unwrap().running_cos.push_back(co);
                    } else { // Some async operations required.
                        let state = self.state.clone();
                        let begin = p.build_begin();

                        // NLL required for this to work
                        begin.run(OnceInvokeBox::new(move |()| {
                            state.lock().unwrap().running_cos.push_back(co);
                        }));
                    }
                } else {
                    // The current coroutine is terminated.
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
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn coroutines_should_be_scheduled() {
        let mut sched = Scheduler::new(SchedulerConfig {
            stack_pool: StackPool::new(StackPoolConfig::default())
        });
        let state = sched.state.clone();
    
        sched.state.lock().unwrap().run_coroutine(move |c| {
            let value: Rc<Cell<i32>> = Rc::new(Cell::new(0));
            let value2 = value.clone();
            let p = Promise::new(move |cb| {
                value2.set(42);
                cb.call(());
            });
            assert_eq!(value.get(), 0);
            c.yield_now(&p);
            assert_eq!(value.get(), 42);
            state.lock().unwrap().termination_requested = true;
        });
        if let Err(e) = catch_unwind(AssertUnwindSafe(|| sched.run())) {
            assert_eq!(*e.downcast_ref::<&'static str>().unwrap(), "Termination requested");
        } else {
            unreachable!()
        }
    }
}
