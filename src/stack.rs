use platform;

pub struct Stack {
    mem: *mut [u8]
}

unsafe impl Send for Stack {}

impl Stack {
    pub fn new(stack_size: usize) -> Stack {
        // Allocate one more page as the guard page
        let mem = platform::setup_stack(stack_size + *platform::PAGE_SIZE);
        unsafe {
            platform::setup_stack_guard_page(mem);
        }
        Stack {
            mem: mem
        }
    }

    pub fn initial_rsp(&self) -> usize {
        let mem = unsafe { &mut *self.mem };
        &mut mem[0] as *mut u8 as usize + mem.len()
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        unsafe {
            platform::free_stack(self.mem);
        }
    }
}
