use std::cell::UnsafeCell;
use co::{CommonCoState, SendableCoState};
use scheduler::{SharedSchedState, SyncSchedState};
use invoke_box::OnceInvokeBox;

pub enum PromiseState {
    Waiting(OnceInvokeBox<NotifyHandle, ()>),
    Started // running / terminated
}

/// Promises act as an async "primitive" that does not carry data.
pub struct Promise {
    // TODO: change to Cell
    state: UnsafeCell<PromiseState>
}

pub struct PromiseBegin {
    target: OnceInvokeBox<NotifyHandle, ()>
}

pub struct NotifyHandle {
    sched_state: SharedSchedState,
    co: Box<CommonCoState>
}

pub struct SendableNotifyHandle {
    sched_state: SyncSchedState,
    co: SendableCoState
}

impl NotifyHandle {
    pub(crate) fn new(s: SharedSchedState, co: Box<CommonCoState>) -> NotifyHandle {
        NotifyHandle {
            sched_state: s,
            co: co
        }
    }

    pub fn notify(self) {
        self.sched_state.push_coroutine_raw(self.co);
    }

    pub fn into_sendable(self) -> SendableNotifyHandle {
        SendableNotifyHandle {
            sched_state: self.sched_state.get_sync(),
            co: SendableCoState::new(self.co)
        }
    }
}

impl SendableNotifyHandle {
    // TODO: Is this correct?
    pub fn notify(self) {
        // Safe as long as we've made sure that the coroutine originally
        // belongs to the sched_state
        unsafe {
            self.sched_state.add_coroutine(self.co);
        }
    }

    fn _assert_sendable(self) {
        let _: Box<Send> = Box::new(self);
    }
}

impl PromiseBegin {
    pub fn run(self, cb: NotifyHandle) {
        self.target.call(cb)
    }
}

impl Promise {
    pub fn new<F: FnOnce(NotifyHandle) + 'static>(f: F) -> Promise {
        Promise {
            state: UnsafeCell::new(PromiseState::Waiting(
                OnceInvokeBox::new(f)
            ))
        }
    }

    pub fn new_started() -> Promise {
        Promise {
            state: UnsafeCell::new(PromiseState::Started)
        }
    }

    pub fn is_started(&self) -> bool {
        let state = unsafe { &mut *self.state.get() };
        match *state {
            PromiseState::Waiting(_) => false,
            PromiseState::Started => true
        }
    }

    pub fn build_begin(&self) -> PromiseBegin {
        let state = unsafe { &mut *self.state.get() };
        match *state {
            PromiseState::Waiting(_) => {},
            _ => panic!("Attempting to call begin() on an already started promise")
        }
        if let PromiseState::Waiting(t) = ::std::mem::replace(state, PromiseState::Started) {
            PromiseBegin {
                target: t
            }
        } else {
            unreachable!()
        }
    }
}
