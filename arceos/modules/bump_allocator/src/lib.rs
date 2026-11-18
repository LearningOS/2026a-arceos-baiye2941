#![no_std]

use allocator::{BaseAllocator, ByteAllocator, PageAllocator};

/// Early memory allocator
/// Use it before formal bytes-allocator and pages-allocator can work!
/// This is a double-end memory range:
/// - Alloc bytes forward
/// - Alloc pages backward
///
/// [ bytes-used | avail-area | pages-used ]
/// |            | -->    <-- |            |
/// start       b_pos        p_pos       end
///
/// For bytes area, 'count' records number of allocations.
/// When it goes down to ZERO, free bytes-used area.
/// For pages area, it will never be freed!
///
pub struct EarlyAllocator<const SIZE: usize> {
    start: usize,
    end: usize,
    b_pos: usize,
    p_pos: usize,
    count: usize,
}

impl<const SIZE: usize> EarlyAllocator<SIZE> {
    pub const fn new() -> Self {
        Self {
            start: 0,
            end: 0,
            b_pos: 0,
            p_pos: 0,
            count: 0,
        }
    }
}

impl<const SIZE: usize> BaseAllocator for EarlyAllocator<SIZE> {
    fn init(&mut self, start: usize, size: usize) {
        self.start = start;
        self.end = start + size;
        self.b_pos = start;
        self.p_pos = start + size;
        self.count = 0;
    }

    fn add_memory(&mut self, start: usize, size: usize) -> allocator::AllocResult {
        if self.start == 0 && self.end == 0 {
            self.init(start, size);
            Ok(())
        } else {
            Err(allocator::AllocError::NoMemory)
        }
    }
}

impl<const SIZE: usize> ByteAllocator for EarlyAllocator<SIZE> {
    fn alloc(
        &mut self,
        layout: core::alloc::Layout,
    ) -> allocator::AllocResult<core::ptr::NonNull<u8>> {
        let align = layout.align();

        let aligned = self
            .b_pos
            .checked_add(align - 1)
            .map(|pos| pos & !(align - 1))
            .ok_or(allocator::AllocError::NoMemory)?;

        let new_pos = aligned
            .checked_add(layout.size())
            .ok_or(allocator::AllocError::NoMemory)?;

        if new_pos > self.p_pos {
            return Err(allocator::AllocError::NoMemory);
        }

        self.b_pos = new_pos;
        self.count += 1;
        
        core::ptr::NonNull::new(aligned as *mut u8)
            .ok_or(allocator::AllocError::NoMemory)
    }
    fn dealloc(&mut self, pos: core::ptr::NonNull<u8>, layout: core::alloc::Layout) {
        self.count = self.count.saturating_sub(1);
        if self.count == 0 {
            self.b_pos = self.start;
        }
    }

    fn total_bytes(&self) -> usize {
        self.end - self.start
    }

    fn used_bytes(&self) -> usize {
        self.b_pos - self.start
    }

    fn available_bytes(&self) -> usize {
        self.p_pos - self.b_pos
    }
}

impl<const SIZE: usize> PageAllocator for EarlyAllocator<SIZE> {
    const PAGE_SIZE: usize = SIZE;

    fn alloc_pages(
        &mut self,
        num_pages: usize,
        align_pow2: usize,
    ) -> allocator::AllocResult<usize> {
        if num_pages == 0 || !align_pow2.is_power_of_two() {
            return Err(allocator::AllocError::InvalidParam);
        }

        let size = num_pages.checked_mul(SIZE)
            .ok_or(allocator::AllocError::NoMemory)?;
        
        let end = self.p_pos
            .checked_add(size)
            .ok_or(allocator::AllocError::NoMemory)?;
        
        let pos = end & !(align_pow2 - 1);
        
        if pos < self.p_pos {
            return Err(allocator::AllocError::NoMemory);
        }
        
        self.p_pos = pos;
        
        Ok(pos)
    }

    fn dealloc_pages(&mut self, pos: usize, num_pages: usize) {
        
    }

    fn total_pages(&self) -> usize {
        (self.end - self.start) / SIZE
    }

    fn used_pages(&self) -> usize {
        (self.end - self.p_pos) / SIZE
    }

    fn available_pages(&self) -> usize {
        (self.p_pos - self.b_pos) / SIZE
    }
}
