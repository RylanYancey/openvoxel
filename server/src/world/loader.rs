//! Types for facilitating the loading and saving of chunks from packed region files.
//!
//! There is one file per region. All 256 chunks are packed into that one file.
//!
//! Each chunk is compressed individually. The entire region IS NOT compressed as one
//! huge block. This is to allow partial loads of regions.
//!
//! To access region data without loading the entire region file, Memory Mapping (memmap2) is used.
//! The OS will only load the pages of the region file that are accessed.
//!
//! Each chunk has a `Segment` that describes its page offset (start), and number of pages (count).
//! Each page has a size of 4096 bytes.
//!
//! To manage the memory pages within the file, a Free List Allocator is used. This is basically a
//! linked list of empty blocks sorted by start index. This makes it very cheap to insert, remove,
//! grow, or shrink spans of pages for chunks.
//!
//! Each region will use the file extension `.ovr`, short for 'openvoxel region'

use std::{cmp::Ordering::*, fs, io, ops::Range, path::PathBuf, sync::Arc};

use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task, futures_lite},
};
use bytemuck::{Pod, Zeroable};
use fxhash::FxHashMap;
use memmap2::{MmapMut, MmapOptions};
use protocol::bytes::Bytes;
use world::{
    Region, World,
    region::{RegionId, chunk::ChunkId, format::ZippedChunk},
};
use zip::{Algorithm, ZipLevel};

use crate::events::RegionLoaded;

/// Resource for loading and saving regions.
#[derive(Resource)]
pub struct WorldLoader {
    /// Location of region data in the file system.
    region_dir: Arc<PathBuf>,

    /// Max size, in bytes, of a zipped chunk.
    chunk_size_limit: u32,

    /// Algorithm to use during compression.
    algorithm: Algorithm,

    /// Compression level to use, currently only respected by Zstd.
    zip_level: ZipLevel,

    /// Map of currently loaded region files.
    /// So long as a Region exists in the World, it must have
    /// an entry in this map. However, an entry in this map
    /// can exist without existing in the World.
    loaded: FxHashMap<RegionId, RegionFile>,

    /// Priority Queue of regions to be loaded into the World.
    /// This is dequeued just by iterating and taking the max prio.
    queue: FxHashMap<RegionId, LoadTask>,
}

impl WorldLoader {
    pub fn algorithm(&self) -> Algorithm {
        self.algorithm
    }

    pub fn zip_level(&self) -> ZipLevel {
        self.zip_level
    }

    /// Request a region to be loaded, with a distance to determine priority.
    ///
    /// The distance should be the chebyshev distance from the requesting player
    /// and the chunk within the region that triggered the request.
    ///
    /// Returns "true" only if the region was added to the queue by this call.
    ///
    /// Does nothing if the region is already in-progress.
    pub fn open_region(&mut self, id: impl Into<RegionId>, distance: u32) -> bool {
        let id = id.into();
        let priority = 8 - (distance >> 9).min(7);
        if let Some(task) = self.queue.get_mut(&id) {
            if let LoadTask::Pending(prio) = task {
                *prio += priority;
            }

            false
        } else {
            self.queue.insert(id, LoadTask::Pending(priority));
            true
        }
    }

    /// Closes and saves the region, returning 'false' if the region did not exist.
    pub fn close_region(&mut self, id: impl Into<RegionId>) -> bool {
        self.loaded.remove(&id.into()).is_some()
    }

    /// Check whether a region is loaded.
    pub fn is_loaded(&mut self, id: impl Into<RegionId>) -> bool {
        self.loaded.contains_key(&id.into())
    }

    /// Flush region changes to disk.
    pub fn save_region(&self, id: impl Into<RegionId>) -> bool {
        if let Some(file) = self.loaded.get(&id.into()) {
            file.save_all();
            true
        } else {
            false
        }
    }

    /// Read raw, compressed chunk data as a slice.
    pub fn read_chunk_raw(&self, id: impl Into<ChunkId>) -> Result<&[u8], ChunkReadError> {
        let id = id.into();
        if let Some(file) = self.loaded.get(&id.to_region_id()) {
            let raw = file.read_segment(id.to_chunk_idx());
            if raw.len() == 0 {
                Err(ChunkReadError::NoData)
            } else {
                Ok(raw)
            }
        } else {
            Err(ChunkReadError::RegionNotLoaded)
        }
    }

    /// Read compressed chunk data.
    pub fn read_chunk(&self, id: impl Into<ChunkId>) -> Result<ZippedChunk, ChunkReadError> {
        let data = self.read_chunk_raw(id)?;
        Ok(ZippedChunk(Bytes::from(data.to_vec())))
    }

    /// Write compressed chunk data to file.
    /// Does not flush.
    pub fn write_chunk_raw(
        &mut self,
        id: impl Into<ChunkId>,
        data: &[u8],
    ) -> Result<(), ChunkWriteError> {
        if data.len() > self.chunk_size_limit as usize {
            return Err(ChunkWriteError::TooLarge);
        }

        let id = id.into();
        if let Some(file) = self.loaded.get_mut(&id.to_region_id()) {
            file.write_segment(id.to_chunk_idx(), &data);
            Ok(())
        } else {
            Err(ChunkWriteError::RegionNotLoaded)
        }
    }

    /// Write compressed chunk data.
    pub fn write_chunk(
        &mut self,
        id: impl Into<ChunkId>,
        data: &ZippedChunk,
    ) -> Result<(), ChunkWriteError> {
        self.write_chunk_raw(id, &data.0)
    }

    fn process_queues(&mut self, evs: &mut MessageWriter<RegionLoaded>, world: &mut World) {
        let mut max_prio: Option<(RegionId, u32)> = None;

        // dequeue completed region load tasks and find highest priority pending task.
        for (id, task) in self.queue.extract_if(|id, task| match task {
            LoadTask::Pending(prio) => {
                if max_prio.is_none_or(|(_, curr_max)| *prio > curr_max) {
                    max_prio = Some((*id, *prio));
                }
                false
            }
            LoadTask::Running(task) => task.is_finished(),
        }) {
            match task {
                LoadTask::Pending(_) => unreachable!(),
                LoadTask::Running(task) => match futures_lite::future::block_on(task) {
                    Err(e) => panic!("[S777] Failed to load region: '{e:?}'"),
                    Ok(file) => {
                        info!("FINISHED LOADING REGION: {}", id.as_ivec2());
                        let header = file.header();
                        let region = Box::new(Region::new(header.origin, header.height as i32));
                        evs.write(RegionLoaded(id));
                        world.insert(region);
                        self.loaded.insert(id, file);
                    }
                },
            }
        }

        // start the region load task for the highest priority pending task
        // that was found in the previous step.
        if let Some((id, _)) = max_prio {
            let task_pool = IoTaskPool::get();
            if let Some(task) = self.queue.get_mut(&id) {
                info!("STARTED LOADING REGION: {}", id.as_ivec2());
                *task = LoadTask::Running(task_pool.spawn(RegionFile::load_async(
                    self.region_dir.clone(),
                    id.as_ivec3(world.min_y()),
                    world.height(),
                )));
            }
        }
    }
}

impl Default for WorldLoader {
    fn default() -> Self {
        Self {
            region_dir: Arc::new(PathBuf::from("/home/wade/Documents/test-save-data/")),
            chunk_size_limit: 1_000_000,
            algorithm: Algorithm::Zstd,
            zip_level: ZipLevel::default(),
            loaded: FxHashMap::default(),
            queue: FxHashMap::default(),
        }
    }
}

pub fn process_loader_queues(
    mut loader: ResMut<WorldLoader>,
    mut world: ResMut<World>,
    mut evs: MessageWriter<RegionLoaded>,
) {
    loader.process_queues(&mut evs, &mut world);
}

#[derive(Debug, Clone)]
pub enum ChunkReadError {
    /// The containing region of the chunk did not exist.
    RegionNotLoaded,

    /// There are no data associated with this chunk.
    NoData,
}

#[derive(Debug, Clone)]
pub enum ChunkWriteError {
    /// The chunk was too large.
    TooLarge,

    /// Containing region of chunk did not exist.
    RegionNotLoaded,
}

enum LoadTask {
    Pending(u32),
    Running(Task<io::Result<RegionFile>>),
}

/// Writes data on drop.
struct RegionFile {
    /// Handle to actual file.
    file: fs::File,

    /// Number of non-header pages in the file.
    /// The number of bytes is given by (page_count as u64 + 1) << 12
    page_count: u16,

    /// Memory-mapped read/writer.
    map: MmapMut,
}

impl RegionFile {
    async fn load_async(path: Arc<PathBuf>, origin: IVec3, height: i32) -> io::Result<RegionFile> {
        Self::load(path, origin, height)
    }

    fn load(path: Arc<PathBuf>, origin: IVec3, height: i32) -> io::Result<RegionFile> {
        // get the path of the region file by computing the morton code of the XZ origin.
        let path = path.join(str::from_utf8(&filename(origin.xz())).unwrap());

        // attempt to load the file.
        match fs::OpenOptions::new()
            .create(false)
            .write(true)
            .read(true)
            .open(&path)
        {
            // file opened, already exists.
            Ok(file) => {
                // read file size to validate it.
                let size = fs::metadata(&path)?.len();

                // construct mmap
                let mut ret = Self {
                    map: unsafe { MmapOptions::new().len(size as usize).map_mut(&file)? },
                    page_count: ((size >> 12) as u16).saturating_sub(1),
                    file,
                };

                // If there is not enough data in the file,
                // clear it because it is corrupted.
                if size <= 4096 {
                    ret.file.set_len(4096)?;
                    ret.init_header(origin, height);
                }

                // TODO: validate header

                return Ok(ret);
            }

            // file not found, create.
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let file = fs::OpenOptions::new()
                    .create_new(true)
                    .read(true)
                    .write(true)
                    .open(&path)?;
                file.set_len(4096)?;

                let mut ret = Self {
                    map: unsafe { MmapOptions::new().len(4096).map_mut(&file)? },
                    page_count: 0,
                    file,
                };

                ret.init_header(origin, height);

                return Ok(ret);
            }

            // operation failed with some other error.
            Err(e) => {
                return Err(e);
            }
        }
    }

    fn init_header(&mut self, origin: IVec3, height: i32) {
        self.map.fill(0);
        self.header_mut().init(origin, height);
    }

    /// Flush all loaded data to disk.
    fn save_all(&self) {
        self.map
            .flush()
            .expect("[S889] Failed to flush region data.")
    }

    /// Flush a segment of the region to disk.
    pub fn save_segment(&self, segment: usize) {
        let range = self.get_segment(segment).byte_range();
        if !range.is_empty() {
            self.map
                .flush_range(range.start, range.len())
                .expect("[S890] Failed to flush region segment.")
        }
    }

    fn header(&self) -> &Header {
        bytemuck::from_bytes(&self.map)
    }

    fn header_mut(&mut self) -> &mut Header {
        bytemuck::from_bytes_mut(&mut self.map)
    }

    /// Check that there is enough space in the file.
    fn check_size(&mut self) {
        if self.page_count < self.header().total_pages {
            self.page_count = self.header().total_pages;
            self.file
                .set_len((self.page_count as u64 + 1) << 12)
                .expect("[S198] Failed to set length of file.");
        }
    }

    /// Write data to the segment, re-allocating if the size changed.
    fn write_segment(&mut self, segment: usize, data: &[u8]) {
        // get the segment, re-allocating if the number of pages has changed.
        let segment = self.header_mut().realloc(segment, data.len());

        // don't copy if data is empty.
        if segment.count != 0 {
            // increase file size if needed.
            self.check_size();

            // copy data to segment range
            self.map[segment.byte_range()].copy_from_slice(data);
        }
    }

    /// Read the bytes at this segment.
    fn read_segment(&self, segment: usize) -> &[u8] {
        &self.map[self.header().segments[segment].byte_range()]
    }

    /// Get the segment at this index.
    fn get_segment(&self, segment: usize) -> Segment {
        self.header().segments[segment]
    }
}

impl Drop for RegionFile {
    fn drop(&mut self) {
        self.save_all();
    }
}

/// Size is 4096 bytes.
#[derive(Copy, Clone, Pod, Zeroable)]
#[repr(C)]
struct Header {
    /// Magic factor for corruption prevention.
    magic: u32,

    /// Version used when creating the region.
    version: u16,

    /// Vertical extent of the region.
    height: i16,

    /// Timestamp of last modification.
    last_modified_at: u64,

    /// Timestamp of creation.
    created_at: u64,

    /// Origin of Region in the World.
    origin: IVec3,

    /// Index of first free node in the linked list.
    free_list_head: u16,

    /// Number of pages large the file is, including
    /// unused pages in between used segments.
    total_pages: u16,

    /// Index of next Block to check for emptyness,
    /// when adding nodes to the free list.
    empty_cursor: u16,

    // reserved for future use.
    _reserved: [u8; 982],

    /// Page segments, one for each chunk.
    segments: [Segment; 256],

    /// Linked-list of free memory blocks.
    ///
    /// This list is sorted by the start index of the block, but not
    /// by its index in the buffer, but by its link order in the list.
    ///
    /// A block with a `start` of u16::MAX is an unused/empty slot.
    free_list: [Block; 256],
}

impl Header {
    const MAGIC: u32 = 0xabcddcba;

    /// Initialize header fields, to be used after zero-filling a new region file.
    fn init(&mut self, origin: IVec3, height: i32) {
        self.magic = Self::MAGIC;
        self.origin = origin;
        self.height = height as i16;
        self.free_list_head = 0;
        self.free_list[0] = Block {
            start: 0,
            count: u16::MAX,
            next: u16::MAX,
        };
        for block in &mut self.free_list[1..] {
            *block = Block::EMPTY;
        }
    }

    /// The number of bytes needed to store the file.
    fn size(&self) -> u64 {
        // page count + 1 (for header) times 4096
        (self.total_pages as u64 + 1) << 12
    }

    /// Loop through the free list until an empty block is found.
    /// Shouldn't result in an infinite loop because it should be
    /// impossible to have 256 blocks.
    fn find_empty_block(&mut self) -> usize {
        let mut i = self.empty_cursor as usize;
        loop {
            if self.free_list[i].is_empty() {
                self.empty_cursor = i as u16;
                return i;
            }

            i = (i + 1) & 255;
        }
    }

    fn alloc(&mut self, len: usize) -> Range<usize> {
        // just to be sure
        assert_ne!(len, 0);

        let count = len as u16;
        let mut curr = self.free_list_head as usize;
        let mut prev: Option<usize> = None;

        // keep searching until a block with available space is found.
        loop {
            let block = self.free_list[curr];

            match block.count.cmp(&count) {
                // block not large enough.
                Less => {}

                // block count equal to count, Perfect fit.
                // Remove the block entirely.
                Equal => {
                    // remove curr
                    self.free_list[curr] = Block::EMPTY;
                    if let Some(prev) = prev {
                        // curr not head, make prev point to next.
                        self.free_list[prev].next = block.next;
                    } else {
                        // curr is head, set head to next.
                        self.free_list_head = block.next;

                        // Next=max indicates a corrupted free list because if
                        // there is only one node in the list, it has a count of u16::MAX,
                        // so it cannot be a perfect fit.
                        assert_ne!(block.next, u16::MAX);
                    }
                }

                // block count greater than count, leaves space
                // remaining in block.
                Greater => {
                    self.free_list[curr].take_end(count);
                    return self.free_list[curr].range();
                }
            }

            if block.next == u16::MAX {
                panic!("Corrupted free list")
            }

            curr = block.next as usize;
            prev = Some(curr);
        }
    }

    fn free(&mut self, segment: Segment) {
        let mut curr = self.free_list_head as usize;
        let mut prev: Option<usize> = None;

        loop {
            let block = self.free_list[curr];

            if block.start > segment.start {
                // the segment is in-between curr and prev.

                if segment.end() == block.start {
                    // the segment is bordering on curr.
                    if let Some(prev) = prev
                        && self.free_list[prev].end() == segment.start
                    {
                        // segment borders BOTH prev and curr, merge curr, prev, and segment into prev
                        // by removing curr, extending prev, and pointing prev to curr.next.
                        self.free_list[prev].push_end(segment.count + block.count);
                        self.free_list[prev].next = block.next;
                    } else {
                        // curr is either head or does not border prev.
                        // segment borders curr, but not prev, just extend curr back.
                        self.free_list[curr].push_start(segment.count);
                    }
                } else {
                    // the segment does not border curr.
                    if let Some(prev) = prev {
                        // curr is not head.
                        if self.free_list[prev].end() == segment.start {
                            // segment borders prev, but not curr.
                            self.free_list[prev].push_end(segment.count);
                        } else {
                            // segment is entirely within prev and curr, so
                            // create a new block pointing to curr and make
                            // prev point to the new block.
                            let idx = self.find_empty_block();
                            self.free_list[idx] = segment.block(curr as u16);
                            self.free_list[prev].next = block.next;
                        }
                    } else {
                        // curr is head, make a new block as head that points to curr.
                        let idx = self.find_empty_block();
                        self.free_list[idx] = segment.block(curr as u16);
                        self.free_list[curr].next = u16::MAX;
                        self.free_list_head = idx as u16;
                    }
                }

                return;
            }

            if block.next == u16::MAX {
                unreachable!("Corrupted free list.")
            }

            curr = block.next as usize;
            prev = Some(curr);
        }
    }

    /// Change the size of the segment at this index to `new_size` bytes.
    /// `new_size` can be smaller, larger, the same size, or 0. segment size
    /// can be zero.
    fn realloc(&mut self, segment: usize, new_size: usize) -> Segment {
        let new_len = (new_size >> 12) + 1;
        let new_count = new_len as u16;
        let seg = self.segments[segment];

        let range = match new_count.cmp(&seg.count) {
            // nothing changed
            Equal => seg.range(),

            // shrink
            Less => {
                if new_count == 0 {
                    self.free(seg);
                    0..0
                } else {
                    self.shrink(seg, new_len);
                    seg.subrange(new_count)
                }
            }

            // grow
            Greater => {
                if seg.count == 0 {
                    self.alloc(new_len)
                } else {
                    self.grow(seg, new_len)
                }
            }
        };

        self.total_pages = u16::max(self.total_pages, range.end as u16);

        let segment = &mut self.segments[segment];
        segment.start = range.start as u16;
        segment.count = new_count;
        segment.update_padding(new_size);
        *segment
    }

    /// Grow the size of an existing segment to a new length. new_len must be
    /// greater than the segment.count. segment.count must be nonzero.
    ///
    /// Does not update the segment range or total_pages. If total_pages changes,
    /// you will need to detect it and update the file accordingly.
    ///
    /// The returned range may be overlapping with the existing range.
    fn grow(&mut self, segment: Segment, new_len: usize) -> Range<usize> {
        let new_len = new_len as u16;
        let change = new_len - segment.count;

        // just to be sure
        assert!(new_len > segment.count);

        // current free list cursor
        let mut curr = self.free_list_head as usize;
        let mut prev: Option<usize> = None;

        // index of best block found so far
        let mut best_block_len = u16::MAX;
        let mut best_block: Option<usize> = None;
        let mut best_prev: Option<usize> = None;

        // Keep going until the containing block is found
        loop {
            let block = self.free_list[curr];

            if block.start > segment.start {
                // At this point, we know segment is in-between curr and prev.
                // We need to either advance the segment into the block, or free the
                // segment and find another suitable block.

                if segment.end() == block.start {
                    // End of segment touches start of curr free block, so
                    // we can advance the segment into the free block OR
                    // remove it entirely if the grow would fully occupy
                    // the space.

                    match block.count.cmp(&change) {
                        // Free Block count in curr is equal to change.
                        // The grow would fully occupy the block, so
                        // we need to remove the block from the free list.
                        Equal => {
                            if let Some(prev) = prev {
                                self.free_list[prev].next = block.next;
                            } else {
                                self.free_list_head = block.next;
                            }

                            self.free_list[curr] = Block::EMPTY;
                            return segment.subrange(new_len);
                        }

                        // Free Block count in curr is greater than change.
                        // the grow would leave some space available in
                        // the block, so we just need to shrink the block
                        // to make the space owned by the segment.
                        Greater => {
                            self.free_list[curr].start += change;
                            self.free_list[curr].count -= change;
                            return segment.subrange(new_len);
                        }

                        // Free Block count in curr is less than change.
                        // the block does not have enough space to grow, free the
                        // entire segment and break to find another spot.
                        Less => {
                            if let Some(prev) = prev
                                && self.free_list[prev].end() == segment.start
                            {
                                // segment borders prev and next free block, extend
                                // prev past the segment and curr. Removes curr and points
                                // prev to curr.next.
                                self.free_list[prev].push_end(block.count + segment.count);
                                self.free_list[prev].next = block.next;
                                self.free_list[curr] = Block::EMPTY;
                            } else {
                                // segment borders next and either does not
                                // border previous or prev does not exist, so
                                // we just need to advance curr backwards.
                                self.free_list[curr].push_start(block.count);
                            }
                        }
                    }
                } else {
                    // Segment is in-between prev and curr, but there is occupied
                    // space between the segment end and curr start that can't be
                    // grown into. So we need to free the segment.

                    if let Some(prev) = prev {
                        if self.free_list[prev].end() == segment.start {
                            // prev exists and the segment borders it, so
                            // we need to extend prev into the segment.
                            self.free_list[prev].count += segment.count;
                        } else {
                            // prev exists, but segment does not border it.
                            // insert a new node in between prev and curr.
                            let idx = self.find_empty_block();
                            self.free_list[idx] = segment.block(curr as u16);
                            self.free_list[prev].next = idx as u16;
                        }
                    } else {
                        // curr is head, so create a new block for the freed space.
                        let idx = self.find_empty_block();
                        self.free_list[self.free_list_head as usize].next = u16::MAX;
                        self.free_list[idx] = segment.block(self.free_list_head);
                        self.free_list_head = idx as u16;
                    }
                }

                break;
            }

            if block.count >= new_len && block.count < best_block_len {
                best_block_len = block.count;
                best_block = Some(curr);
                best_prev = prev;
            }

            // should always find the next block before
            // end of list because last element has a count
            // of u16::MAX.
            debug_assert_ne!(block.next, u16::MAX);

            // advance to next node
            curr = block.next as usize;
            prev = Some(curr);
        }

        // This is reachable if the `break` of the loop is reached, indicating the
        // segment was freed and now we need to find the best-fitting slot for the new size.

        // Keep searching until a suitable block is found.
        // This won't go on forever as long as the end of the list has a count of u16::MAX.
        while best_block.is_none() {
            let block = self.free_list[curr];
            if block.count >= new_len {
                best_block = Some(curr);
                best_prev = prev;
                break;
            }

            // should always find a best block before
            // end of list because last element has a count
            // of u16::MAX.
            debug_assert_ne!(block.next, u16::MAX);

            curr = block.next as usize;
            prev = Some(curr);
        }

        // We know that best_block is some because of the previous loop.
        let index = best_block.unwrap();
        let block = self.free_list[index];
        let ret = block.subrange(new_len);

        match block.count.cmp(&new_len) {
            // Perfect fit, remove the block from the list.
            Equal => {
                self.free_list[index] = Block::EMPTY;
                if let Some(prev) = best_prev {
                    // block has prev, point prev to block.next
                    self.free_list[prev].next = block.next;
                } else {
                    // block is head, point head to next.
                    self.free_list_head = block.next;
                }
            }

            // Space is left in the upper portion of the block,
            // just increment past the new length.
            Greater => self.free_list[index].push_end(new_len),

            // Unreachable because our best_block loop only
            // terminates once a block with enough size is found.
            Less => panic!("corrupted free list."),
        }

        ret
    }

    /// Shrink a segment to the new page size by decrementing the count.
    ///
    /// new_len must be less than the current length of the segment,
    /// and must be greater than zero.
    ///
    /// Does not update the segment, that is up to you.
    fn shrink(&mut self, segment: Segment, new_len: usize) {
        let new_len = new_len as u16;
        let mut curr = self.free_list_head as usize;
        let mut prev: Option<usize> = None;

        // the number of blocks we are shrinking by.
        let change = segment.count - new_len;

        // Keep going until a block is found whose start is after segment's start.
        loop {
            let block = self.free_list[curr];

            // stop on first node with start greater than or eq to segment.
            // When this happens, the segment is in-between curr and previous.
            if block.start > segment.start {
                // The removed segment may be touching the start of curr, but it cannot
                // be touching the end of prev because shrink enforces that the
                // previous size and new size be non-zero.

                if segment.end() == block.start {
                    // segment ends at the start of the block,
                    // so we need to push to the start of the block.
                    self.free_list[curr].push_start(change);
                } else {
                    // Removed segment does not touch a boundary
                    // of any existing block, a new free block
                    // needs to be inserted.

                    // get index of available slot.
                    let idx = self.find_empty_block();

                    // write new free block pointing to curr.next.
                    self.free_list[idx] = Block {
                        start: segment.start + new_len,
                        count: change,
                        next: self.free_list[curr].next,
                    };

                    if let Some(prev) = prev {
                        // if prev exists, curr is not head.
                        self.free_list[prev].next = idx as u16;
                    } else {
                        // if prev does not exist, this is now head.
                        self.free_list_head = idx as u16;
                    }
                }

                return;
            }

            if block.next == u16::MAX {
                panic!("Corrupted free list")
            }

            curr = block.next as usize;
            prev = Some(curr);
        }
    }
}

/// A segment describing the memory location of a chunk's data in the file.
#[derive(Copy, Clone, Pod, Zeroable)]
#[repr(C)]
struct Segment {
    /// The first page in the segment.
    start: u16,

    /// The number of pages in the segment.
    count: u16,

    /// The number of bytes between the end
    /// of the chunk's data and the end of
    /// its last page.
    padding: u16,
}

impl Segment {
    const fn end(&self) -> u16 {
        self.start + self.count
    }

    const fn block(&self, next: u16) -> Block {
        Block {
            start: self.start,
            count: self.count,
            next,
        }
    }

    const fn subrange(&self, count: u16) -> Range<usize> {
        (self.start as usize)..(self.start + count) as usize
    }

    const fn range(&self) -> Range<usize> {
        (self.start as usize)..(self.start + self.count) as usize
    }

    const fn byte_range(&self) -> Range<usize> {
        let start = (self.start as usize + 1) << 12;
        start..(start + (((self.count as usize) << 12) - self.padding as usize))
    }

    fn update_padding(&mut self, size: usize) {
        self.padding = 4096 - ((size & 4095) as u16)
    }
}

/// A free block of memory.
#[derive(Copy, Clone, Pod, Zeroable)]
#[repr(C)]
struct Block {
    /// The first page in the free block.
    start: u16,

    /// The number of pages in the block.
    count: u16,

    /// The index of the next free block.
    next: u16,
}

impl Block {
    const EMPTY: Self = Self {
        start: u16::MAX,
        count: u16::MAX,
        next: u16::MAX,
    };

    const fn is_empty(&self) -> bool {
        self.start == u16::MAX
    }

    const fn end(&self) -> u16 {
        self.start + self.count
    }

    fn range(&self) -> Range<usize> {
        (self.start as usize)..(self.start + self.count) as usize
    }

    /// Shift the start forward by cnt pages,
    /// decrease the length by cnt pages.
    const fn take_start(&mut self, cnt: u16) {
        self.start += cnt;
        self.count -= cnt;
    }

    /// Shift the end backward by cnt pages,
    /// decrease the length by cnt pages.
    const fn take_end(&mut self, cnt: u16) {
        self.count -= cnt;
    }

    /// Shift the start backward by cnt pages,
    /// increase the length by cnt pages.
    const fn push_start(&mut self, cnt: u16) {
        self.start -= cnt;
        self.count += cnt;
    }

    /// Shift the end forward by cnt pages,
    /// increase the length by cnt pages.
    const fn push_end(&mut self, cnt: u16) {
        self.count += cnt;
    }

    fn subrange(&self, len: u16) -> Range<usize> {
        (self.start as usize)..(self.start + len) as usize
    }
}

/// Region Filename as an array of UTF-8 bytes.
/// Hex digits are the zorder index of the Regions XZ origin.
/// "x" + "<12 hex digits>" + ".ovr"
fn filename(origin: IVec2) -> [u8; 17] {
    const HEX_DIGITS: [u8; 16] = [
        b'0', b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'a', b'b', b'c', b'd', b'e',
        b'f',
    ];

    // the upper 18 bits of this number are always 0, leaving us with 46 relevant bits.
    let mut zorder = zorder::index_of([(origin.x as u32) >> 9, (origin.y as u32) >> 9]);
    let mut ret = [0u8; 17];

    // I'm including 'x' at the start of the filename, just because idk what
    // happens when a file has a number as its first char. Then I encode the ext at the end.
    ret[0] = b'x';
    ret[13..17].copy_from_slice(&[b'.', b'o', b'v', b'r']);

    // visit zorder 4 bits at a time.
    for i in 1..13 {
        ret[i] = HEX_DIGITS[(zorder & 0xF) as usize];
        zorder >>= 4;
    }

    ret
}
