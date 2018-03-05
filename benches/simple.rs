#![feature(test)]

extern crate liblightning;
extern crate test;

use std::cell::Cell;
use liblightning::{CoState, Stack, StackPool, StackPoolConfig};
use test::Bencher;

#[bench]
fn bench_yield(b: &mut Bencher) {
    let mut co = CoState::new(Stack::new(16384), |c| {
        loop {
            let cont: Cell<bool> = Cell::new(true);
            c.yield_now(&cont);
            if !cont.get() {
                break;
            }
        }
    });
    b.iter(|| {
        co.resume().unwrap();
    });
    co.resume().unwrap().downcast_ref::<Cell<bool>>()
        .unwrap()
        .set(false);
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
