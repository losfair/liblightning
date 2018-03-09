extern crate liblightning;

use std::thread;
use std::time::Duration;
use std::sync::mpsc;
use std::rc::Rc;
use std::cell::Cell;
use liblightning::Scheduler;
use liblightning::Promise;
use liblightning::Yieldable;

fn sleep_ms(c: &mut Yieldable, ms: u64) {
    let p = Promise::new(move |h| {
        let h = h.into_sendable();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(ms));
            h.notify();
        });
    });
    c.yield_now(&p);
}

fn main() {
    let mut sched = Scheduler::new_default();
    let state = sched.get_state();
    let state2 = state.clone();

    sched.run_value_promise_to_end(state.prepare_coroutine(move |c| {
        let done_count: Rc<Cell<usize>> = Rc::new(Cell::new(0));
        let done_count_2 = done_count.clone();

        let p = Promise::new(move |handle| {
            // TODO: Implement Promise::all

            let handle = Rc::new(Cell::new(Some(handle)));
            let handle2 = handle.clone();

            state2.start_coroutine(move |c| {
                println!("Begin 1");
                sleep_ms(c, 500);
                println!("End 1");
                done_count.set(done_count.get() + 1);
                if done_count.get() == 2 {
                    handle.replace(None).unwrap().notify();
                }
            });

            state2.start_coroutine(move |c| {
                println!("Begin 2");
                sleep_ms(c, 500);
                println!("End 2");
                done_count_2.set(done_count_2.get() + 1);
                if done_count_2.get() == 2 {
                    handle2.replace(None).unwrap().notify();
                }
            });
        });

        c.yield_now(&p);
    })).unwrap();
}
