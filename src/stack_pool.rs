use std::cell::RefCell;
use stack::Stack;

pub struct StackPool {
    stacks: RefCell<Vec<Stack>>,
    config: StackPoolConfig
}

pub struct StackPoolConfig {
    pub default_stack_size: usize,
    pub max_pool_size: usize
}

impl Default for StackPoolConfig {
    fn default() -> Self {
        StackPoolConfig {
            default_stack_size: 32768,
            max_pool_size: 4096
        }
    }
}

impl StackPool {
    pub fn new(config: StackPoolConfig) -> StackPool {
        StackPool {
            stacks: RefCell::new(Vec::new()),
            config: config
        }
    }

    pub fn get(&self) -> Stack {
        match self.stacks.borrow_mut().pop() {
            Some(v) => v,
            None => Stack::new(self.config.default_stack_size)
        }
    }

    pub fn put(&self, s: Stack) {
        let mut stacks = self.stacks.borrow_mut();
        if self.config.max_pool_size == 0 || stacks.len() < self.config.max_pool_size {
            stacks.push(s);
        }
    }
}
