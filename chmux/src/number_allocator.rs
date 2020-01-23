use std::collections::HashSet;

/// Allocates numbers randomly and uniquely.
pub struct NumberAllocator {
    used: HashSet<u32>,
}

/// Number allocated exhausted.
#[derive(Debug, Clone)]
pub struct NumberAllocatorExhaustedError;

impl NumberAllocator {
    /// Creates a new number allocator.
    pub fn new() -> NumberAllocator {
        NumberAllocator {
            used: HashSet::new(),
        }
    }

    /// Allocates a random, unique number.
    pub fn allocate(&mut self) -> Result<u32, NumberAllocatorExhaustedError> {
        if self.used.len() >= (std::u32::MAX / 2) as usize {
            return Err(NumberAllocatorExhaustedError);
        }
        loop {
            let cand = rand::random();
            if !self.used.contains(&cand) {
                self.used.insert(cand);
                return Ok(cand);
            }
        }
    }

    /// Releases a previously allocated number.
    /// Panics when the number is currently not allocated.
    pub fn release(&mut self, number: u32) {
        if !self.used.remove(&number) {
            panic!("NumberAllocator cannot release number {} that is currently not allocated.", number);
        }
    }
}
