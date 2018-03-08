use std::cell::Cell;

pub struct OnceInvokeBox<A1, R> {
    inner: Box<Fn(A1) -> R>
}

impl<A1, R> OnceInvokeBox<A1, R> {
    pub fn call(self, a1: A1) -> R {
        (self.inner)(a1)
    }

    pub fn new<F: FnOnce(A1) -> R + 'static>(f: F) -> OnceInvokeBox<A1, R> {
        let target: Cell<Option<F>> = Cell::new(Some(f));

        OnceInvokeBox {
            inner: Box::new(move |a1| {
                (target.replace(None).unwrap())(a1)
            })
        }
    }
}
