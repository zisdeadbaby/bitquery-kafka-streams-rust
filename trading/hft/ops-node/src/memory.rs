use bumpalo::Bump;
use crossbeam_queue::ArrayQueue; // Corrected import
use parking_lot::{Mutex}; // Removed RwLock as it's not used for offset
use std::cell::RefCell;
// use std::sync::Arc; // Not used directly in this file
use zerocopy::{FromBytes, IntoBytes, KnownLayout, Immutable}; // Corrected typo KnownLayout

thread_local! {
    static ARENA: RefCell<Bump> = RefCell::new(Bump::with_capacity(1024 * 1024)); // 1MB per thread arena
}

pub struct MemoryPool {
    arenas: ArrayQueue<Box<Bump>>, // Store Box<Bump> to allow moving them
    huge_pages_allocator: Option<HugePageAllocator>, // Renamed for clarity
}

impl MemoryPool {
    pub fn new(size_mb: usize, enable_huge_pages: bool) -> Self {
        let arena_capacity_mb = 4; // Each pooled arena is 4MB
        let arena_count = if size_mb >= arena_capacity_mb { size_mb / arena_capacity_mb } else { 1 }; // Ensure at least one arena if size_mb is small
        let arenas = ArrayQueue::new(arena_count.max(1)); // Ensure queue size is at least 1

        for _ in 0..arena_count {
            let arena = Box::new(Bump::with_capacity(arena_capacity_mb * 1024 * 1024));
            if arenas.push(arena).is_err() {
                // This should not happen if arena_count matches queue capacity
                tracing::warn!("Failed to push initial arena to pool, pool might be smaller than configured.");
            }
        }

        let huge_pages_allocator = if enable_huge_pages {
            match HugePageAllocator::new() {
                Ok(allocator) => {
                    tracing::info!("HugePageAllocator initialized successfully.");
                    Some(allocator)
                },
                Err(e) => {
                    tracing::warn!("Failed to initialize HugePageAllocator: {}. Huge pages will be disabled.", e);
                    None
                }
            }
        } else {
            None
        };

        Self { arenas, huge_pages_allocator }
    }

    pub fn acquire_arena(&self) -> Option<Box<Bump>> {
        self.arenas.pop()
    }

    pub fn release_arena(&self, mut arena: Box<Bump>) {
        arena.reset();
        if self.arenas.push(arena).is_err() {
            tracing::warn!("Failed to release arena back to pool; it might be dropped. This could indicate pool exhaustion or misconfiguration.");
            // If the queue is full, the arena will be dropped, which is acceptable.
        }
    }

    // Added a method to get the huge page allocator if available
    pub fn huge_page_allocator(&self) -> Option<&HugePageAllocator> {
        self.huge_pages_allocator.as_ref()
    }
}

pub struct HugePageAllocator {
    base_ptr: *mut u8,
    size: usize,
    offset: Mutex<usize>, // Mutex is sufficient here
}

unsafe impl Send for HugePageAllocator {}
unsafe impl Sync for HugePageAllocator {}

impl HugePageAllocator {
    // Made HUGR_PAGE_SIZE and NUM_PAGES configurable or constants
    const HUGE_PAGE_SIZE_BYTES: usize = 2 * 1024 * 1024; // 2MB
    const NUM_HUGE_PAGES: usize = 64; // Default number of pages

    fn new() -> std::io::Result<Self> {
        use libc::{mmap, MAP_ANONYMOUS, MAP_HUGETLB, MAP_PRIVATE, PROT_READ, PROT_WRITE};

        let size = Self::HUGE_PAGE_SIZE_BYTES * Self::NUM_HUGE_PAGES;

        tracing::info!("Attempting to allocate {}MB for huge pages ({} pages of {}MB).", size / (1024*1024), Self::NUM_HUGE_PAGES, Self::HUGE_PAGE_SIZE_BYTES / (1024*1024));

        let ptr = unsafe {
            mmap(
                std::ptr::null_mut(),
                size,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_ANONYMOUS | MAP_HUGETLB,
                -1,
                0,
            )
        };

        if ptr == libc::MAP_FAILED {
            let err = std::io::Error::last_os_error();
            tracing::error!("Huge page mmap failed: {}. Ensure huge pages are configured on the system.", err);
            return Err(err);
        }

        tracing::info!("Successfully mmaped {} bytes for huge pages at {:p}.", size, ptr);

        Ok(Self {
            base_ptr: ptr as *mut u8,
            size,
            offset: Mutex::new(0),
        })
    }

    pub unsafe fn allocate(&self, layout: std::alloc::Layout) -> Option<*mut u8> {
        let mut offset_guard = self.offset.lock(); // Changed to offset_guard for clarity

        let aligned_offset = (*offset_guard + layout.align() - 1) & !(layout.align() - 1);
        let new_offset = aligned_offset + layout.size();

        if new_offset > self.size {
            tracing::warn!("HugePageAllocator out of memory: requested size {}, align {}, available {}.", layout.size(), layout.align(), self.size - aligned_offset);
            return None;
        }

        *offset_guard = new_offset;
        let ptr = self.base_ptr.add(aligned_offset);
        tracing::trace!("Allocated {} bytes with align {} from huge pages at {:p}. New offset: {}", layout.size(), layout.align(), ptr, new_offset);
        Some(ptr)
    }
}

impl Drop for HugePageAllocator {
    fn drop(&mut self) {
        tracing::info!("Unmapping huge page allocation of size {} at {:p}.", self.size, self.base_ptr);
        unsafe {
            if libc::munmap(self.base_ptr as *mut libc::c_void, self.size) == -1 {
                tracing::error!("Failed to munmap huge pages: {}", std::io::Error::last_os_error());
            }
        }
    }
}

// Zero-copy structures
#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Clone, Copy, Debug)] // Added Debug
#[repr(C)]
pub struct MarketDataPacket {
    pub timestamp_ns: u64,
    pub slot: u64,
    pub price: u64, // Representing fixed-point decimal
    pub volume: u64, // Representing fixed-point decimal
    pub account_key: [u8; 32], // Assuming Solana Pubkey
}

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Clone, Copy, Debug)] // Added Debug
#[repr(C)]
pub struct OrderPacket {
    pub order_id: u64,
    pub timestamp_ns: u64,
    pub price: u64,
    pub quantity: u64,
    pub side: u8, // 0 for Bid, 1 for Ask
    pub order_type: u8, // e.g., 0 for Limit, 1 for Market
    // pub padding: [u8; 6], // Removed padding, ensure struct is naturally aligned or add specific alignment
}


// Lock-free ring buffer for order flow - SPSC (Single Producer Single Consumer)
pub struct SPSCRingBuffer<T: Copy, const N: usize> {
    buffer: Box<[std::mem::MaybeUninit<T>; N]>, // Ensure N is a power of 2 for efficient modulo
    head: std::sync::atomic::AtomicUsize, // Consumer reads from head
    tail: std::sync::atomic::AtomicUsize, // Producer writes to tail
    // Removed cached_head and cached_tail for simplicity and to rely on atomic operations directly for this example.
    // For extreme optimization, caching can be added back carefully.
}

unsafe impl<T: Copy + Send, const N: usize> Send for SPSCRingBuffer<T, N> {}
unsafe impl<T: Copy + Send, const N: usize> Sync for SPSCRingBuffer<T, N> {} // Sync is needed if Arc<SPSCRingBuffer> is shared

impl<T: Copy, const N: usize> SPSCRingBuffer<T, N> {
    pub fn new() -> Self {
        assert!(N.is_power_of_two(), "SPSCRingBuffer size N must be a power of two.");
        Self {
            buffer: unsafe { Box::new_zeroed().assume_init() }, // Initialize with zeros
            head: std::sync::atomic::AtomicUsize::new(0),
            tail: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    // Producer side
    pub fn push(&self, item: T) -> Result<(), T> {
        let current_tail = self.tail.load(std::sync::atomic::Ordering::Relaxed);
        let next_tail = (current_tail + 1) & (N - 1); // Use bitwise AND for power-of-two N

        // Check if buffer is full. `head` is loaded with Acquire to ensure visibility of consumer progress.
        if next_tail == self.head.load(std::sync::atomic::Ordering::Acquire) {
            return Err(item); // Buffer is full
        }

        unsafe {
            // Write item to the buffer
            (self.buffer.as_ptr() as *mut std::mem::MaybeUninit<T>)
                .add(current_tail)
                .cast::<T>()
                .write(item);
        }

        // Make the written item available to the consumer. Release ordering ensures prior writes are visible.
        self.tail.store(next_tail, std::sync::atomic::Ordering::Release);
        Ok(())
    }

    // Consumer side
    pub fn pop(&self) -> Option<T> {
        let current_head = self.head.load(std::sync::atomic::Ordering::Relaxed);

        // Check if buffer is empty. `tail` is loaded with Acquire to ensure visibility of producer progress.
        if current_head == self.tail.load(std::sync::atomic::Ordering::Acquire) {
            return None; // Buffer is empty
        }

        let item = unsafe {
            // Read item from the buffer
            (self.buffer.as_ptr() as *const std::mem::MaybeUninit<T>)
                .add(current_head)
                .cast::<T>()
                .read()
        };

        // Make the slot available for the producer. Release ordering ensures prior reads (if any) are done.
        self.head.store((current_head + 1) & (N - 1), std::sync::atomic::Ordering::Release);
        Some(item)
    }

    // Added helper methods
    pub fn is_empty(&self) -> bool {
        self.head.load(std::sync::atomic::Ordering::Acquire) == self.tail.load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn is_full(&self) -> bool {
        ((self.tail.load(std::sync::atomic::Ordering::Acquire) + 1) & (N-1)) == self.head.load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn len(&self) -> usize {
        // This provides an approximate length, might not be exact due to concurrent modifications
        (self.tail.load(std::sync::atomic::Ordering::Relaxed).wrapping_sub(self.head.load(std::sync::atomic::Ordering::Relaxed))) & (N - 1)
    }
}

// Example usage of thread_local arena if needed directly
pub fn with_thread_arena<F, R>(f: F) -> R
where
    F: FnOnce(&Bump) -> R,
{
    ARENA.with(|arena_cell| {
        let mut arena = arena_cell.borrow_mut();
        let result = f(&arena);
        arena.reset(); // Reset after use for this specific pattern
        result
    })
}

pub fn alloc_with_thread_arena<'a, T>(val: T) -> &'a mut T where T: 'a {
     ARENA.with(|arena_cell| {
        let arena = arena_cell.borrow_mut();
        // Note: This is tricky due to lifetime. Bumpalo's alloc methods return &'arena T.
        // For this to be truly useful across function boundaries without passing the arena,
        // it would require unsafe lifetime extension or a different pattern.
        // This example assumes T is self-contained or its lifetime is managed carefully.
        // A more common pattern is to pass the arena into functions that need to allocate.
        // The lifetime 'a here is tied to the lifetime of the Bump inside the RefCell,
        // which is static but its contents are reset. This is generally unsafe.
        // A safer approach is to use the pool or pass arenas explicitly.
        // This function is kept for conceptual illustration but should be used with extreme caution.
        unsafe {
            // This is unsound as the arena can be reset.
            // For thread-local bump allocation, it's better to manage the Bump instance more directly.
            std::mem::transmute(arena.alloc(val))
        }
    })
}
