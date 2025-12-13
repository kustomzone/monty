use std::fmt;
use std::time::{Duration, Instant};

/// Error returned when a resource limit is exceeded during execution.
///
/// This allows the sandbox to enforce strict limits on allocation count,
/// execution time, and memory usage.
#[derive(Debug, Clone)]
pub enum ResourceError {
    /// Maximum number of allocations exceeded.
    Allocation { limit: usize, count: usize },
    /// Maximum execution time exceeded.
    Time { limit: Duration, elapsed: Duration },
    /// Maximum memory usage exceeded.
    Memory { limit: usize, used: usize },
}

impl fmt::Display for ResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Allocation { limit, count } => {
                write!(f, "allocation limit exceeded: {count} > {limit}")
            }
            Self::Time { limit, elapsed } => {
                write!(f, "time limit exceeded: {elapsed:?} > {limit:?}")
            }
            Self::Memory { limit, used } => {
                write!(f, "memory limit exceeded: {used} bytes > {limit} bytes")
            }
        }
    }
}

impl std::error::Error for ResourceError {}

/// Trait for tracking resource usage and scheduling garbage collection.
///
/// Implementations can enforce limits on allocations, time, and memory,
/// as well as schedule periodic garbage collection.
///
/// All implementations should eventually trigger garbage collection to handle
/// reference cycles. The `should_gc` method controls *frequency*, not whether
/// GC runs at all.
pub trait ResourceTracker: fmt::Debug {
    /// Called before each heap allocation.
    ///
    /// Returns `Ok(())` if the allocation should proceed, or `Err(ResourceError)`
    /// if a limit would be exceeded.
    ///
    /// # Arguments
    /// * `size` - Approximate size in bytes of the allocation
    fn on_allocate(&mut self, get_size: impl FnOnce() -> usize) -> Result<(), ResourceError>;

    /// Called when memory is freed (during dec_ref or garbage collection).
    ///
    /// # Arguments
    /// * `size` - Size in bytes of the freed allocation
    fn on_free(&mut self, get_size: impl FnOnce() -> usize);

    /// Called periodically (at statement boundaries) to check time limits.
    ///
    /// Returns `Ok(())` if within time limit, or `Err(ResourceError::Time)`
    /// if the limit is exceeded.
    fn check_time(&self) -> Result<(), ResourceError>;

    /// Returns true if garbage collection should run.
    ///
    /// Called at statement boundaries where we have access to GC roots.
    ///
    ///  Note: GC won't run during long-running single expressions (e.g., large list
    /// comprehensions). This is acceptable because most Python code is structured
    /// as multiple statements, and resource limits (time, memory) still apply.
    fn should_gc(&self) -> bool;

    /// Called after garbage collection completes.
    ///
    /// Used to reset internal counters (e.g., allocations since last GC).
    fn on_gc_complete(&mut self);
}

/// Default GC interval for `NoLimitTracker` - run GC every 100,000 allocations.
///
/// This is intentionally very infrequent to minimize overhead while still
/// eventually collecting reference cycles.
const DEFAULT_GC_INTERVAL: usize = 100_000;

/// A resource tracker that imposes no limits but still triggers infrequent GC.
///
/// This tracker does not enforce any resource limits (allocations, time, memory),
/// but still triggers garbage collection periodically to collect reference cycles.
/// GC runs every 100,000 allocations by default.
#[derive(Debug, Default, Clone)]
pub struct NoLimitTracker {
    /// Number of allocations since last garbage collection.
    allocations_since_gc: usize,
}

impl ResourceTracker for NoLimitTracker {
    #[inline]
    fn on_allocate(&mut self, _: impl FnOnce() -> usize) -> Result<(), ResourceError> {
        self.allocations_since_gc += 1;
        Ok(())
    }

    #[inline]
    fn on_free(&mut self, _: impl FnOnce() -> usize) {}

    #[inline]
    fn check_time(&self) -> Result<(), ResourceError> {
        Ok(())
    }

    #[inline]
    fn should_gc(&self) -> bool {
        self.allocations_since_gc >= DEFAULT_GC_INTERVAL
    }

    #[inline]
    fn on_gc_complete(&mut self) {
        self.allocations_since_gc = 0;
    }
}

/// Configuration for resource limits.
///
/// All limits are optional - set to `None` to disable a specific limit.
/// Use `ResourceLimits::default()` for no limits, or build custom limits
/// with the builder pattern.
#[derive(Debug, Clone, Default)]
pub struct ResourceLimits {
    /// Maximum number of heap allocations allowed.
    pub max_allocations: Option<usize>,
    /// Maximum execution time.
    pub max_duration: Option<Duration>,
    /// Maximum heap memory in bytes (approximate).
    pub max_memory: Option<usize>,
    /// Run garbage collection every N allocations.
    pub gc_interval: Option<usize>,
}

impl ResourceLimits {
    /// Creates a new ResourceLimits with all limits disabled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the maximum number of allocations.
    #[must_use]
    pub fn max_allocations(mut self, limit: usize) -> Self {
        self.max_allocations = Some(limit);
        self
    }

    /// Sets the maximum execution duration.
    #[must_use]
    pub fn max_duration(mut self, limit: Duration) -> Self {
        self.max_duration = Some(limit);
        self
    }

    /// Sets the maximum memory usage in bytes.
    #[must_use]
    pub fn max_memory(mut self, limit: usize) -> Self {
        self.max_memory = Some(limit);
        self
    }

    /// Sets the garbage collection interval (run GC every N allocations).
    #[must_use]
    pub fn gc_interval(mut self, interval: usize) -> Self {
        self.gc_interval = Some(interval);
        self
    }
}

/// A resource tracker that enforces configurable limits.
///
/// Tracks allocation count, memory usage, and execution time, returning
/// errors when limits are exceeded. Also schedules garbage collection
/// at configurable intervals.
#[derive(Debug)]
pub struct LimitedTracker {
    limits: ResourceLimits,
    /// When execution started (for time limit checking).
    start_time: Instant,
    /// Total number of allocations made.
    allocation_count: usize,
    /// Current approximate memory usage in bytes.
    current_memory: usize,
    /// Number of allocations since last garbage collection.
    allocations_since_gc: usize,
}

impl LimitedTracker {
    /// Creates a new LimitedTracker with the given limits.
    ///
    /// The start time is recorded when the tracker is created, so create
    /// it immediately before starting execution.
    #[must_use]
    pub fn new(limits: ResourceLimits) -> Self {
        Self {
            limits,
            start_time: Instant::now(),
            allocation_count: 0,
            current_memory: 0,
            allocations_since_gc: 0,
        }
    }

    /// Returns the current allocation count.
    #[must_use]
    pub fn allocation_count(&self) -> usize {
        self.allocation_count
    }

    /// Returns the current approximate memory usage.
    #[must_use]
    pub fn current_memory(&self) -> usize {
        self.current_memory
    }

    /// Returns the elapsed time since tracker creation.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }
}

impl ResourceTracker for LimitedTracker {
    fn on_allocate(&mut self, get_size: impl FnOnce() -> usize) -> Result<(), ResourceError> {
        // Check allocation count limit
        if let Some(max) = self.limits.max_allocations {
            if self.allocation_count >= max {
                return Err(ResourceError::Allocation {
                    limit: max,
                    count: self.allocation_count + 1,
                });
            }
        }

        let size = get_size();
        // Check memory limit
        if let Some(max) = self.limits.max_memory {
            let new_memory = self.current_memory + size;
            if new_memory > max {
                return Err(ResourceError::Memory {
                    limit: max,
                    used: new_memory,
                });
            }
        }

        // Update tracking state
        self.allocation_count += 1;
        self.current_memory += size;
        self.allocations_since_gc += 1;

        Ok(())
    }

    fn on_free(&mut self, get_size: impl FnOnce() -> usize) {
        self.current_memory = self.current_memory.saturating_sub(get_size());
    }

    fn check_time(&self) -> Result<(), ResourceError> {
        if let Some(max) = self.limits.max_duration {
            let elapsed = self.start_time.elapsed();
            if elapsed > max {
                return Err(ResourceError::Time { limit: max, elapsed });
            }
        }
        Ok(())
    }

    fn should_gc(&self) -> bool {
        self.limits
            .gc_interval
            .is_some_and(|interval| self.allocations_since_gc >= interval)
    }

    fn on_gc_complete(&mut self) {
        self.allocations_since_gc = 0;
    }
}
