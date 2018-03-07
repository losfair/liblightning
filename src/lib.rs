#![feature(nll)]

extern crate libc;
#[macro_use]
extern crate lazy_static;

pub mod co;
pub mod stack;
pub mod stack_pool;
pub mod scheduler;
mod platform;

pub use co::{CoState, Promise, CommonPromise};
pub use stack::Stack;
pub use stack_pool::{StackPool, StackPoolConfig};
