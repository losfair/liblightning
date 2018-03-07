#![feature(test)]

extern crate liblightning;
extern crate test;

use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use liblightning::{CoState, Stack, StackPool, StackPoolConfig, Promise};
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
