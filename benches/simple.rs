#![feature(test)]

extern crate liblightning;
extern crate test;

use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::panic::resume_unwind;
use liblightning::{CoState, Stack, StackPool, StackPoolConfig, Promise, Scheduler};
use liblightning::co::CommonCoState;
use test::Bencher;

#[bench]
fn bench_yield(b: &mut Bencher) {
    let flag: Arc<AtomicBool> = Arc::new(AtomicBool::new(true));

    let flag2 = flag.clone();

    let mut co = CoState::new(Stack::new(16384), move |c| {
        loop {
            c.yield_now(&Promise::new_resolved());
            if !flag2.load(Ordering::Relaxed) {
                break;
            }
        }
    });
    b.iter(|| {
        co.resume().unwrap();
    });
    flag.store(false, Ordering::Relaxed);
    assert!(co.resume().is_none());
}

#[bench]
fn bench_run(b: &mut Bencher) {
    let pool = StackPool::new(StackPoolConfig::default());
    b.iter(|| {
        let mut co = CoState::new(pool.get(), |_| ());
        co.resume();
        pool.put(co.take_stack().unwrap());
    })
}

#[bench]
fn bench_sched_run(b: &mut Bencher) {
    let mut sched = Scheduler::new_default();
    let state = sched.get_state();

    b.iter(|| {
        let vp = state.run_coroutine(move |_| {
        });
        let ret = sched.run_value_promise_to_end(vp);
        match ret {
            Ok(_) => {},
            Err(e) => resume_unwind(e)
        }
    });
}

#[bench]
fn bench_sched_yield(b: &mut Bencher) {
    let mut sched = Scheduler::new_default();
    let state = sched.get_state();

    let b = unsafe {
        ::std::mem::transmute::<&mut Bencher, &'static mut Bencher>(b)
    };

    let vp = state.run_coroutine(move |c| {
        b.iter(|| {
            let p = Promise::new_resolved();
            c.yield_now(&p);
        });
    });
    sched.run_value_promise_to_end(vp).unwrap();
}

#[bench]
fn bench_sched_async(b: &mut Bencher) {
    let mut sched = Scheduler::new_default();
    let state = sched.get_state();

    let b = unsafe {
        ::std::mem::transmute::<&mut Bencher, &'static mut Bencher>(b)
    };

    let vp = state.run_coroutine(move |c| {
        b.iter(|| {
            let p = Promise::new(|cb| {
                cb.call(());
            });
            c.yield_now(&p);
        });
    });
    sched.run_value_promise_to_end(vp).unwrap();
}
