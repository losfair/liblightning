extern crate liblightning;

use std::cell::RefCell;
use std::os::raw::c_void;
use liblightning::{Scheduler, Yieldable, Promise, SchedulerConfig, StackPool, StackPoolConfig};
use liblightning::scheduler::SharedSchedState;
use liblightning::promise::{NotifyHandle, SendableNotifyHandle};

thread_local! {
    static YIELD_INFO: RefCell<Vec<*mut Yieldable>> = RefCell::new(Vec::new());
}

pub type CoroutineEntry = extern "C" fn (user_data: *const c_void);
pub type AsyncEntry = extern "C" fn (notify: *mut NotifyHandle, user_data: *const c_void);

struct GlobalYieldableGuard {

}

impl GlobalYieldableGuard {
    fn new(c: *mut Yieldable) -> GlobalYieldableGuard {
        YIELD_INFO.with(|cc| {
            cc.borrow_mut().push(c);
        });
        GlobalYieldableGuard {}
    }
}

impl Drop for GlobalYieldableGuard {
    fn drop(&mut self) {
        YIELD_INFO.with(|cc| {
            cc.borrow_mut().pop().unwrap();
        });
    }
}

#[no_mangle]
pub extern "C" fn ll_scheduler_new() -> *mut Scheduler {
    Box::into_raw(Box::new(Scheduler::new_default()))
}

#[no_mangle]
pub extern "C" fn ll_scheduler_new_with_config(stack_size: usize, max_pool_size: usize) -> *mut Scheduler {
    Box::into_raw(Box::new(Scheduler::new(SchedulerConfig {
        stack_pool: StackPool::new(StackPoolConfig {
            default_stack_size: stack_size,
            max_pool_size: max_pool_size
        })
    })))
}

#[no_mangle]
pub unsafe extern "C" fn ll_scheduler_destroy(sch: *mut Scheduler) {
    Box::from_raw(sch);
}

#[no_mangle]
pub extern "C" fn ll_scheduler_get_state(sch: &mut Scheduler) -> *mut SharedSchedState {
    Box::into_raw(Box::new(sch.get_state()))
}

#[no_mangle]
pub extern "C" fn ll_scheduler_run(sch: &mut Scheduler) {
    sch.run();
}

#[no_mangle]
pub extern "C" fn ll_scheduler_run_once(sch: &mut Scheduler, max_run_count: usize) -> usize {
    sch.run_once(max_run_count)
}

#[no_mangle]
pub extern "C" fn ll_scheduler_start_coroutine(
    sch: &mut Scheduler,
    entry: CoroutineEntry,
    user_data: *const c_void
) {
    sch.get_state().start_coroutine(move |c| {
        let c = unsafe {
            ::std::mem::transmute::<&mut Yieldable, *mut Yieldable>(c)
        };
        let _guard = GlobalYieldableGuard::new(c);
        entry(user_data);
    });
}

#[no_mangle]
pub unsafe extern "C" fn ll_shared_state_destroy(
    s: *mut SharedSchedState
) {
    Box::from_raw(s);
}

#[no_mangle]
pub unsafe extern "C" fn ll_current_stack_end() -> *mut u8 {
    YIELD_INFO.with(|info| {
        let info = info.borrow();
        (&*info[info.len() - 1]).stack_end()
    })
}

#[no_mangle]
pub unsafe extern "C" fn ll_async_enter(
    cb: AsyncEntry,
    user_data: *const c_void
) {
    let raw_yieldable = YIELD_INFO.with(|info| info.borrow_mut().pop().unwrap());
    let yieldable = &mut *raw_yieldable;

    let p = Promise::new(move |notify| {
        cb(Box::into_raw(Box::new(notify)), user_data);
    });
    yieldable.yield_now(&p);

    YIELD_INFO.with(|info| info.borrow_mut().push(raw_yieldable));
}

#[no_mangle]
pub unsafe extern "C" fn ll_async_exit(
    notify: *mut NotifyHandle
) {
    let notify = Box::from_raw(notify);
    notify.notify();
}

#[no_mangle]
pub unsafe extern "C" fn ll_async_notify_into_sendable(
    handle: *mut NotifyHandle
) -> *mut SendableNotifyHandle {
    let handle = Box::from_raw(handle);
    Box::into_raw(Box::new(handle.into_sendable()))
}

#[no_mangle]
pub unsafe extern "C" fn ll_async_exit_sendable(
    notify: *mut SendableNotifyHandle
) {
    let notify = Box::from_raw(notify);
    notify.notify();
}
