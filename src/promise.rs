use std::cell::UnsafeCell;
use invoke_box::OnceInvokeBox;

pub enum PromiseState {
    Waiting(OnceInvokeBox<OnceInvokeBox<(), ()>, ()>),
    Pending,
    Resolved
}

/// Promises act as an async "primitive" that does not carry data.
pub struct Promise {
    // TODO: change to Cell
    state: UnsafeCell<PromiseState>
}

pub struct PromiseBegin {
    target: OnceInvokeBox<OnceInvokeBox<(), ()>, ()>
}

impl PromiseBegin {
    pub fn run(self, cb: OnceInvokeBox<(), ()>) {
        self.target.call(cb)
    }
}

impl Promise {
    pub fn new<F: FnOnce(OnceInvokeBox<(), ()>) + 'static>(f: F) -> Promise {
        Promise {
            state: UnsafeCell::new(PromiseState::Waiting(
                OnceInvokeBox::new(f)
            ))
        }
    }

    pub fn new_resolved() -> Promise {
        Promise {
            state: UnsafeCell::new(PromiseState::Resolved)
        }
    }

    pub fn resolve(&self) {
        unsafe {
            let state = &mut *self.state.get();

            match *state {
                PromiseState::Resolved => panic!("Attempting to resolve an already resolved promise"),
                _ => {}
            }

            *self.state.get() = PromiseState::Resolved;
        }
    }

    pub fn is_resolved(&self) -> bool {
        let state = unsafe { &*self.state.get() };
        match *state {
            PromiseState::Waiting(_) | PromiseState::Pending => false,
            PromiseState::Resolved => true
        }
    }

    pub fn build_begin(&self) -> PromiseBegin {
        let state = unsafe { &mut *self.state.get() };
        match *state {
            PromiseState::Waiting(_) => {},
            _ => panic!("Attempting to call begin() on an already started promise")
        }
        if let PromiseState::Waiting(t) = ::std::mem::replace(state, PromiseState::Pending) {
            PromiseBegin {
                target: t
            }
        } else {
            unreachable!()
        }
    }
}
