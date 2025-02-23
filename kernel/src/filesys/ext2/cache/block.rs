use alloc::{collections::BTreeMap, sync::Arc, vec, vec::Vec};
use spin::Mutex;

use super::{
    super::block_io::BlockIO, Cache, CacheEntry, CacheError, CacheResult, CacheStats,
    CacheableItem, Clock, MonotonicClock,
};

/// A cached block with its data and metadata
#[derive(Clone)]
pub struct CachedBlock {
    data: Vec<u8>,
    dirty: bool,
}

impl CachedBlock {
    fn new(size: usize) -> Self {
        Self {
            data: vec![0; size],
            dirty: false,
        }
    }

    /// Get a reference to the block's data
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get a mutable reference to the block's data
    pub fn data_mut(&mut self) -> &mut [u8] {
        self.mark_dirty();
        &mut self.data
    }
}

impl CacheableItem for CachedBlock {
    fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn mark_clean(&mut self) {
        self.dirty = false;
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}

/// Block cache implementation
pub struct BlockCache {
    // The underlying block device
    device: Arc<dyn BlockIO>,

    // Cache entries mapped by block number
    entries: Mutex<BTreeMap<u32, CacheEntry<CachedBlock>>>,

    // Maximum number of cached blocks
    capacity: usize,

    // Cache statistics
    stats: Mutex<CacheStats>,

    // Clock for entry aging
    clock: MonotonicClock,
}

impl BlockCache {
    /// Create a new block cache
    pub fn new(device: Arc<dyn BlockIO>, capacity: usize) -> Self {
        Self {
            device,
            entries: Mutex::new(BTreeMap::new()),
            capacity,
            stats: Mutex::default(),
            clock: MonotonicClock::default(),
        }
    }

    /// Find the least recently used entry that isn't currently in use
    fn find_lru_entry(&self, entries: &mut BTreeMap<u32, CacheEntry<CachedBlock>>) -> Option<u32> {
        entries
            .iter()
            .filter(|(_, entry)| Arc::strong_count(&entry.value) == 1) // Only consider blocks we alone reference
            .min_by_key(|(_, entry)| (entry.last_access, entry.access_count))
            .map(|(block, _)| *block)
    }

    /// Load a block from the device
    fn load_block(&self, block: u32) -> CacheResult<CachedBlock> {
        let mut cached = CachedBlock::new(self.device.block_size() as usize);
        self.device
            .read_block(block, &mut cached.data)
            .map_err(|_| CacheError::LoadError)?;
        Ok(cached)
    }

    /// Write a block back to the device
    fn write_block(&self, block: u32, cached: &CachedBlock) -> CacheResult<()> {
        if !cached.is_dirty() {
            return Ok(());
        }

        self.device
            .write_block(block, &cached.data)
            .map_err(|_| CacheError::WriteError)?;
        Ok(())
    }

    /// Evict entries if cache is full
    fn evict_if_needed(
        &self,
        entries: &mut BTreeMap<u32, CacheEntry<CachedBlock>>,
    ) -> CacheResult<()> {
        if entries.len() < self.capacity {
            return Ok(());
        }

        if let Some(block) = self.find_lru_entry(entries) {
            let entry = entries.remove(&block).unwrap();

            // We know we can lock this since ref_count = 1
            let guard = entry.value.lock();

            // Write back if dirty
            if guard.is_dirty() {
                self.write_block(block, &guard)?;
                self.stats.lock().writebacks += 1;
            }

            self.stats.lock().evictions += 1;
            Ok(())
        } else {
            Err(CacheError::CacheFull)
        }
    }
}

impl Cache<u32, CachedBlock> for BlockCache {
    fn get(&self, block: u32) -> CacheResult<Arc<Mutex<CachedBlock>>> {
        let mut entries = self.entries.lock();
        let now = self.clock.now();

        if let Some(entry) = entries.get_mut(&block) {
            entry.touch(now);
            self.stats.lock().hits += 1;
            return Ok(Arc::clone(&entry.value));
        }

        self.stats.lock().misses += 1;

        self.evict_if_needed(&mut entries)?;

        let cached = self.load_block(block)?;
        let entry = CacheEntry::new(cached);
        let value = Arc::clone(&entry.value);
        entries.insert(block, entry);

        Ok(value)
    }

    fn insert(&self, block: u32, cached: CachedBlock) -> CacheResult<()> {
        let mut entries = self.entries.lock();

        self.evict_if_needed(&mut entries)?;

        // Add new entry
        entries.insert(block, CacheEntry::new(cached));
        Ok(())
    }

    fn remove(&self, block: &u32) -> CacheResult<()> {
        let mut entries = self.entries.lock();

        if let Some(entry) = entries.remove(block) {
            let guard = entry.value.lock();
            // Write back if dirty
            if guard.is_dirty() {
                self.write_block(*block, &guard)?;
                self.stats.lock().writebacks += 1;
            }
        }

        Ok(())
    }

    fn clear(&self) -> CacheResult<()> {
        let mut entries = self.entries.lock();

        // Write back all dirty blocks
        for (block, entry) in entries.iter() {
            let guard = entry.value.lock();
            if guard.is_dirty() {
                self.write_block(*block, &guard)?;
                self.stats.lock().writebacks += 1;
            }
        }

        entries.clear();
        Ok(())
    }

    fn stats(&self) -> CacheStats {
        self.stats.lock().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{super::super::block_io::MockDevice, *};
    use alloc::sync::Arc;

    // Test CachedBlock functionality
    #[test_case]
    fn test_cached_block_basic() {
        let mut block = CachedBlock::new(1024);
        assert!(!block.is_dirty());

        // Modify data should mark dirty
        block.data_mut()[0] = 42;
        assert!(block.is_dirty());

        // Reading shouldn't affect dirty state
        assert_eq!(block.data()[0], 42);
        assert!(block.is_dirty());

        block.mark_clean();
        assert!(!block.is_dirty());
    }

    // Test BlockCache operations
    #[test_case]
    fn test_block_cache_basic_operations() {
        let device: Arc<dyn BlockIO> = MockDevice::new(1024, 1024 * 1024); // 1MB device
        let cache = BlockCache::new(Arc::clone(&device), 2); // Cache 2 blocks

        // Write to a block
        {
            let block = cache.get(0).unwrap();
            let mut block = block.lock();
            block.data_mut()[0] = 42;
        }

        // Read it back
        {
            let block = cache.get(0).unwrap();
            let block = block.lock();
            assert_eq!(block.data()[0], 42);
        }

        // Stats should show one hit, one miss
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
    }

    #[test_case]
    fn test_block_cache_eviction() {
        let device: Arc<dyn BlockIO> = MockDevice::new(1024, 1024 * 1024);
        let cache = BlockCache::new(Arc::clone(&device), 2);

        // Fill cache but don't keep references
        cache.get(0).unwrap();
        cache.get(1).unwrap();

        // Force eviction by getting third block
        let block2 = cache.get(2).unwrap();
        {
            let mut block = block2.lock();
            block.data_mut()[0] = 42;
        }

        let stats = cache.stats();
        assert!(stats.evictions > 0);
    }

    #[test_case]
    fn test_block_cache_dirty_writeback() {
        let device: Arc<dyn BlockIO> = MockDevice::new(1024, 1024 * 1024);
        let cache = BlockCache::new(Arc::clone(&device), 2);

        // Write to block and mark dirty
        {
            let block = cache.get(0).unwrap();
            let mut block = block.lock();
            block.data_mut()[0] = 42;
        }

        // Clear cache, forcing writeback
        cache.clear().unwrap();

        // Read back from device
        let mut buffer = vec![0; 1024];
        device.read_block(0, &mut buffer).unwrap();
        assert_eq!(buffer[0], 42);

        let stats = cache.stats();
        assert!(stats.writebacks > 0);
    }

    #[test_case]
    fn test_block_cache_concurrent_access() {
        let device: Arc<dyn BlockIO> = MockDevice::new(1024, 1024 * 1024);
        let cache = Arc::new(BlockCache::new(Arc::clone(&device), 4));

        // Get same block multiple times
        let block1 = cache.get(0).unwrap();
        let block2 = cache.get(0).unwrap();

        // Modify through one reference
        {
            let mut guard = block1.lock();
            guard.data_mut()[0] = 42;
        }

        // Read through other reference
        {
            let guard = block2.lock();
            assert_eq!(guard.data()[0], 42);
        }
    }

    #[test_case]
    fn test_block_cache_error_conditions() {
        let device: Arc<dyn BlockIO> = MockDevice::new(1024, 1024 * 1024);
        let cache = BlockCache::new(Arc::clone(&device), 1);

        // Fill cache
        let block = cache.get(0).unwrap();

        // Keep strong reference to prevent eviction
        let mut guard = block.lock();
        guard.data_mut()[0] = 42;

        // Try to get another block - should fail
        assert!(matches!(cache.get(1), Err(CacheError::CacheFull)));
    }

    #[test_case]
    fn test_block_cache_clear() {
        let device: Arc<dyn BlockIO> = MockDevice::new(1024, 1024 * 1024);
        let cache = BlockCache::new(Arc::clone(&device), 2);

        // Add some blocks
        {
            let block = cache.get(0).unwrap();
            let mut block = block.lock();
            block.data_mut()[0] = 42;
        }
        {
            let block = cache.get(1).unwrap();
            let mut block = block.lock();
            block.data_mut()[0] = 43;
        }

        // Clear cache
        cache.clear().unwrap();

        // Verify blocks were written back
        let mut buffer = vec![0; 1024];
        device.read_block(0, &mut buffer).unwrap();
        assert_eq!(buffer[0], 42);

        device.read_block(1, &mut buffer).unwrap();
        assert_eq!(buffer[0], 43);
    }
}
