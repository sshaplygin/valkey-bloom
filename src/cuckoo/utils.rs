use crate::configs;
use bincode::Options;
use cuckoofilter::CuckooFilter as ExternalCuckooFilter;
use cuckoofilter::ExportedCuckooFilter;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::hash::Hasher;
use std::sync::atomic::Ordering;

/// Used for decoding and encoding `CuckooObject`. Must match CUCKOO_TYPE_ENCODING_VERSION in data_type.rs.
pub const CUCKOO_OBJECT_VERSION: u8 = 2;

/// KeySpace Notification Events
pub const ADD_EVENT: &str = "cuckoo.add";
pub const CREATE_EVENT: &str = "cuckoo.create";
pub const RESERVE_EVENT: &str = "cuckoo.reserve";
pub const DEL_EVENT: &str = "cuckoo.del";
pub const INSERT_EVENT: &str = "cuckoo.insert";
pub const LOAD_EVENT: &str = "cuckoo.load";

/// Client Errors
pub const ERROR: &str = "ERROR";
pub const FILTER_FULL: &str = "ERR cuckoo filter is full";
pub const NON_SCALING_FILTER_FULL: &str = "ERR non scaling cuckoo filter is full";
pub const NOT_FOUND: &str = "ERR not found";
pub const ITEM_EXISTS: &str = "ERR item exists";
pub const INVALID_INFO_VALUE: &str = "ERR invalid information value";
pub const BAD_EXPANSION: &str = "ERR bad expansion";
pub const BAD_CAPACITY: &str = "ERR bad capacity";
pub const BAD_BUCKET_SIZE: &str = "ERR bad bucket size";
pub const BAD_MAX_KICKS: &str = "ERR bad max kicks";
pub const BAD_MAX_ITERATIONS: &str = "ERR bad max iterations";
pub const BUCKET_SIZE_RANGE: &str = "ERR (bucket size must be between 1 and 255)";
pub const CAPACITY_LARGER_THAN_0: &str = "ERR (capacity should be larger than 0)";
pub const CAPACITY_OUT_OF_RANGE: &str = "ERR capacity must be between min and max";
pub const CAPACITY_MUST_BE_LARGER_THAN_ZERO: &str = "ERR capacity must be larger than 0";
pub const BUCKET_SIZE_OUT_OF_RANGE: &str = "ERR bucket size must be between min and max";
pub const MAX_KICKS_OUT_OF_RANGE: &str = "ERR max kicks must be between min and max";
pub const CAPACITY_ARG_REQUIRED: &str = "ERR CAPACITY requires an argument";
pub const BUCKET_SIZE_ARG_REQUIRED: &str = "ERR BUCKETSIZE requires an argument";
pub const MAX_ITERATIONS_ARG_REQUIRED: &str = "ERR MAXITERATIONS requires an argument";
pub const EXPANSION_ARG_REQUIRED: &str = "ERR EXPANSION requires an argument";
pub const ITEMS_KEYWORD_REQUIRED: &str = "ERR ITEMS keyword required";
pub const UNKNOWN_OPTION_OR_MISSING_ITEMS: &str = "ERR unknown option or missing ITEMS keyword";
pub const UNKNOWN_ARGUMENT: &str = "ERR unknown argument received";
pub const UNKNOWN_OPTION: &str = "ERR unknown option";
pub const EXCEEDS_MAX_CUCKOO_SIZE: &str = "ERR operation exceeds cuckoo object memory limit";
pub const MAX_NUM_SCALING_FILTERS: &str = "ERR cuckoo object reached max number of filters";
pub const KEY_EXISTS: &str = "BUSYKEY Target key name already exists.";
pub const DECODE_CUCKOO_OBJECT_FAILED: &str = "ERR cuckoo object decoding failed";
pub const DECODE_UNSUPPORTED_VERSION: &str =
    "ERR cuckoo object decoding failed. Unsupported version";
pub const NO_ITEMS_SPECIFIED: &str = "ERR no items specified";
pub const FAILED_TO_SET_FILTER: &str = "ERR failed to set cuckoo filter";

/// Logging Error messages
pub const ENCODE_CUCKOO_OBJECT_FAILED: &str = "Failed to encode cuckoo object.";

/// Max number of filters allowed within a cuckoo object.
pub const CUCKOO_NUM_FILTERS_PER_OBJECT_LIMIT_MAX: i32 = 1024;

pub const MIN_BUCKET_SIZE: usize = 1;
pub const MAX_BUCKET_SIZE: usize = 255;

#[derive(Debug, PartialEq)]
pub enum CuckooError {
    FilterFull,
    NotFound,
    ExceedsMaxSize,
    InvalidParameter,
    SerializationError,
    MaxNumScalingFilters,
    BadCapacity,
    BadBucketSize,
    BadMaxKicks,
    BadExpansion,
    NonScalingFilterFull,
    EncodeFilterFailed,
    DecodeFilterFailed,
    DecodeUnsupportedVersion,
}

impl CuckooError {
    pub fn as_str(&self) -> &'static str {
        match self {
            CuckooError::FilterFull => FILTER_FULL,
            CuckooError::NotFound => NOT_FOUND,
            CuckooError::ExceedsMaxSize => EXCEEDS_MAX_CUCKOO_SIZE,
            CuckooError::InvalidParameter => ERROR,
            CuckooError::SerializationError => ENCODE_CUCKOO_OBJECT_FAILED,
            CuckooError::MaxNumScalingFilters => MAX_NUM_SCALING_FILTERS,
            CuckooError::BadCapacity => BAD_CAPACITY,
            CuckooError::BadBucketSize => BAD_BUCKET_SIZE,
            CuckooError::BadMaxKicks => BAD_MAX_KICKS,
            CuckooError::BadExpansion => BAD_EXPANSION,
            CuckooError::NonScalingFilterFull => NON_SCALING_FILTER_FULL,
            CuckooError::EncodeFilterFailed => ENCODE_CUCKOO_OBJECT_FAILED,
            CuckooError::DecodeFilterFailed => DECODE_CUCKOO_OBJECT_FAILED,
            CuckooError::DecodeUnsupportedVersion => DECODE_UNSUPPORTED_VERSION,
        }
    }
}

/// Top-level CuckooObject structure that can contain multiple filters for scaling
#[allow(clippy::vec_box)]
pub struct CuckooObject {
    expansion: u32,
    bucket_size: usize,
    max_kicks: u32,
    filters: Vec<Box<CuckooFilter>>,
}

impl CuckooObject {
    /// Create a new reserved CuckooObject
    pub fn new_reserved(
        capacity: i64,
        bucket_size: usize,
        max_kicks: u32,
        expansion: u32,
        validate_size_limit: bool,
    ) -> Result<CuckooObject, CuckooError> {
        if !(configs::CUCKOO_CAPACITY_MIN..=configs::CUCKOO_CAPACITY_MAX).contains(&capacity) {
            return Err(CuckooError::BadCapacity);
        }
        if !(MIN_BUCKET_SIZE..=MAX_BUCKET_SIZE).contains(&bucket_size) {
            return Err(CuckooError::BadBucketSize);
        }
        if !(configs::CUCKOO_MAX_KICKS_MIN as u32..=configs::CUCKOO_MAX_KICKS_MAX as u32)
            .contains(&max_kicks)
        {
            return Err(CuckooError::BadMaxKicks);
        }
        if expansion > configs::CUCKOO_EXPANSION_MAX {
            return Err(CuckooError::BadExpansion);
        }
        if validate_size_limit && !CuckooObject::validate_size_before_create(capacity, bucket_size)
        {
            return Err(CuckooError::ExceedsMaxSize);
        }

        let filter = Box::new(CuckooFilter::new(capacity, bucket_size, max_kicks));
        let filters = vec![filter];

        let cuckoo = CuckooObject {
            expansion,
            bucket_size,
            max_kicks,
            filters,
        };

        cuckoo.cuckoo_object_incr_metrics_on_new_create();
        Ok(cuckoo)
    }

    /// Create a CuckooObject from existing data (RDB Load / Restore)
    pub fn from_existing(
        expansion: u32,
        bucket_size: usize,
        max_kicks: u32,
        filters: Vec<Box<CuckooFilter>>,
    ) -> CuckooObject {
        let cuckoo = CuckooObject {
            expansion,
            bucket_size,
            max_kicks,
            filters,
        };

        cuckoo.cuckoo_object_incr_metrics_on_new_create();
        cuckoo
    }

    /// Create a copy of an existing CuckooObject
    pub fn create_copy_from(from: &CuckooObject) -> CuckooObject {
        let mut filters: Vec<Box<CuckooFilter>> = Vec::with_capacity(from.filters.len());
        for filter in &from.filters {
            let new_filter = Box::new(CuckooFilter::create_copy_from(filter));
            filters.push(new_filter);
        }

        let new_copy = CuckooObject {
            expansion: from.expansion,
            bucket_size: from.bucket_size,
            max_kicks: from.max_kicks,
            filters,
        };

        new_copy.cuckoo_object_incr_metrics_on_new_create();
        new_copy
    }

    /// Add an item to the CuckooObject, with auto-scaling if enabled
    pub fn add_item(&mut self, item: &[u8], validate_size_limit: bool) -> Result<i64, CuckooError> {
        if self.item_exists(item) {
            return Ok(1);
        }
        for filter in self.filters.iter_mut().rev() {
            match filter.add(item) {
                Ok(added) => {
                    let _ = added;
                    return Ok(1);
                }
                Err(CuckooError::FilterFull) => {}
                Err(err) => return Err(err),
            }
        }
        if self.expansion == 0 {
            return Err(CuckooError::NonScalingFilterFull);
        }
        if self.filters.len() >= CUCKOO_NUM_FILTERS_PER_OBJECT_LIMIT_MAX as usize {
            return Err(CuckooError::MaxNumScalingFilters);
        }
        let capacity = self
            .filters
            .last()
            .expect("at least one filter")
            .capacity()
            .checked_mul(self.expansion.into())
            .filter(|n| *n <= configs::CUCKOO_CAPACITY_MAX)
            .ok_or(CuckooError::BadCapacity)?;
        if validate_size_limit && !self.validate_size_before_scaling(capacity, self.bucket_size) {
            return Err(CuckooError::ExceedsMaxSize);
        }
        let mut filter = Box::new(CuckooFilter::new(
            capacity,
            self.bucket_size,
            self.max_kicks,
        ));
        filter.add(item)?;
        let before = self.cuckoo_object_memory_usage();
        self.filters.push(filter);
        crate::metrics::CUCKOO_OBJECT_TOTAL_MEMORY_BYTES.fetch_add(
            self.cuckoo_object_memory_usage() - before,
            Ordering::Relaxed,
        );
        Ok(1)
    }

    /// Delete an item from the CuckooObject.
    /// Iterates in reverse so newer (larger) filters are checked first, matching insertion order.
    pub fn delete_item(&mut self, item: &[u8]) -> Result<i64, CuckooError> {
        for filter in self.filters.iter_mut().rev() {
            if filter.delete(item)? {
                return Ok(1);
            }
        }
        Ok(0)
    }

    /// Check if an item exists in any filter
    pub fn item_exists(&self, item: &[u8]) -> bool {
        self.filters.iter().any(|filter| filter.contains(item))
    }

    /// Count occurrences of an item across all filters
    pub fn count_item(&self, item: &[u8]) -> i64 {
        i64::from(self.item_exists(item))
    }

    /// Get total memory usage
    pub fn memory_usage(&self) -> usize {
        let mut mem = self.cuckoo_object_memory_usage();
        for filter in &self.filters {
            mem += filter.number_of_bytes();
        }
        mem
    }

    fn cuckoo_object_memory_usage(&self) -> usize {
        CuckooObject::compute_size(self.filters.capacity())
    }

    pub fn compute_size(filters_vec_capacity: usize) -> usize {
        std::mem::size_of::<CuckooObject>()
            + (filters_vec_capacity * std::mem::size_of::<Box<CuckooFilter>>())
    }

    pub fn capacity(&self) -> i64 {
        self.filters.iter().map(|f| f.capacity()).sum()
    }

    pub fn num_items(&self) -> i64 {
        self.filters.iter().map(|f| f.num_items()).sum()
    }

    pub fn num_filters(&self) -> usize {
        self.filters.len()
    }

    pub fn expansion(&self) -> u32 {
        self.expansion
    }

    pub fn bucket_size(&self) -> usize {
        self.bucket_size
    }

    pub fn max_kicks(&self) -> u32 {
        self.max_kicks
    }

    pub fn starting_capacity(&self) -> i64 {
        self.filters
            .first()
            .expect("Every CuckooObject is expected to have at least one filter")
            .capacity()
    }

    pub fn free_effort(&self) -> usize {
        self.filters.len()
    }

    pub fn filters(&self) -> &Vec<Box<CuckooFilter>> {
        &self.filters
    }

    pub fn filters_mut(&mut self) -> &mut Vec<Box<CuckooFilter>> {
        &mut self.filters
    }

    fn validate_size_before_create(capacity: i64, bucket_size: usize) -> bool {
        let bytes = std::mem::size_of::<CuckooObject>()
            + std::mem::size_of::<Box<CuckooFilter>>()
            + CuckooFilter::compute_size(capacity, bucket_size);
        CuckooObject::validate_size(bytes)
    }

    fn validate_size_before_scaling(&self, new_capacity: i64, bucket_size: usize) -> bool {
        let vector_growth = if self.filters.len() == self.filters.capacity() {
            self.filters.capacity().max(4) * std::mem::size_of::<Box<CuckooFilter>>()
        } else {
            0
        };
        let bytes = self.memory_usage()
            + vector_growth
            + CuckooFilter::compute_size(new_capacity, bucket_size);
        CuckooObject::validate_size(bytes)
    }

    pub fn validate_size(bytes: usize) -> bool {
        bytes <= configs::CUCKOO_MEMORY_LIMIT_PER_OBJECT.load(Ordering::Relaxed) as usize
    }

    pub fn encode_object(&self) -> Result<Vec<u8>, CuckooError> {
        let snapshot = ObjectSnapshot {
            expansion: self.expansion,
            bucket_size: self.bucket_size,
            max_kicks: self.max_kicks,
            filters: self.filters.iter().map(|f| f.snapshot()).collect(),
        };
        let mut bytes = vec![CUCKOO_OBJECT_VERSION];
        bincode::serialize_into(&mut bytes, &snapshot)
            .map_err(|_| CuckooError::EncodeFilterFailed)?;
        Ok(bytes)
    }

    pub fn decode_object(bytes: &[u8], validate_size_limit: bool) -> Result<Self, CuckooError> {
        if bytes.is_empty() {
            return Err(CuckooError::DecodeFilterFailed);
        }
        if bytes[0] != CUCKOO_OBJECT_VERSION {
            return Err(CuckooError::DecodeUnsupportedVersion);
        }
        let snapshot: ObjectSnapshot = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .with_limit(bytes.len() as u64)
            .reject_trailing_bytes()
            .deserialize(&bytes[1..])
            .map_err(|_| CuckooError::DecodeFilterFailed)?;
        if !(MIN_BUCKET_SIZE..=MAX_BUCKET_SIZE).contains(&snapshot.bucket_size) {
            return Err(CuckooError::BadBucketSize);
        }
        if snapshot.max_kicks < configs::CUCKOO_MAX_KICKS_MIN as u32
            || snapshot.max_kicks > configs::CUCKOO_MAX_KICKS_MAX as u32
        {
            return Err(CuckooError::BadMaxKicks);
        }
        if snapshot.expansion > configs::CUCKOO_EXPANSION_MAX {
            return Err(CuckooError::BadExpansion);
        }
        if snapshot.filters.is_empty()
            || snapshot.filters.len() > CUCKOO_NUM_FILTERS_PER_OBJECT_LIMIT_MAX as usize
        {
            return Err(CuckooError::DecodeFilterFailed);
        }
        let mut filters = Vec::with_capacity(1);
        for data in snapshot.filters {
            if !(configs::CUCKOO_CAPACITY_MIN..=configs::CUCKOO_CAPACITY_MAX)
                .contains(&data.capacity)
                || data.length > data.capacity as usize
                || data.values.len()
                    != ExternalFilter::allocation_size(data.capacity as usize, snapshot.bucket_size)
                        .map_err(|_| CuckooError::DecodeFilterFailed)?
                || data.rng_word_pos >= (1u128 << 68)
            {
                return Err(CuckooError::DecodeFilterFailed);
            }
            let filter =
                CuckooFilter::from_snapshot(data, snapshot.bucket_size, snapshot.max_kicks)?;
            filters.push(Box::new(filter));
        }
        let object = Self::from_existing(
            snapshot.expansion,
            snapshot.bucket_size,
            snapshot.max_kicks,
            filters,
        );
        if validate_size_limit && !Self::validate_size(object.memory_usage()) {
            return Err(CuckooError::ExceedsMaxSize);
        }
        Ok(object)
    }

    fn cuckoo_object_incr_metrics_on_new_create(&self) {
        use crate::metrics;
        metrics::CUCKOO_NUM_OBJECTS.fetch_add(1, Ordering::Relaxed);
        metrics::CUCKOO_OBJECT_TOTAL_MEMORY_BYTES
            .fetch_add(self.cuckoo_object_memory_usage(), Ordering::Relaxed);
    }

    fn cuckoo_object_decr_metrics_on_drop(&self) {
        use crate::metrics;
        metrics::CUCKOO_OBJECT_TOTAL_MEMORY_BYTES
            .fetch_sub(self.cuckoo_object_memory_usage(), Ordering::Relaxed);
        metrics::CUCKOO_NUM_OBJECTS.fetch_sub(1, Ordering::Relaxed);
    }
}

impl Drop for CuckooObject {
    fn drop(&mut self) {
        self.cuckoo_object_decr_metrics_on_drop();
    }
}

// SipHash-1-3 with fixed keys and canonical length encoding. Keep hashing
// unchanged for the lifetime of persistence version 2.
#[derive(Clone, Default)]
pub struct FixedHasher(siphasher::sip::SipHasher13);
impl Hasher for FixedHasher {
    fn finish(&self) -> u64 {
        self.0.finish()
    }
    fn write(&mut self, bytes: &[u8]) {
        self.0.write(bytes);
    }
    fn write_usize(&mut self, value: usize) {
        self.write(&(value as u64).to_le_bytes());
    }
}
type ExternalFilter = ExternalCuckooFilter<FixedHasher, ChaCha8Rng>;
const RNG_SEED: u64 = 42;

#[derive(Serialize, Deserialize)]
struct ObjectSnapshot {
    expansion: u32,
    bucket_size: usize,
    max_kicks: u32,
    filters: Vec<FilterSnapshot>,
}

#[derive(Serialize, Deserialize)]
struct FilterSnapshot {
    capacity: i64,
    values: Vec<u8>,
    length: usize,
    rng_word_pos: u128,
}

/// A filter stores fingerprints and RNG state, never the original item bytes.
pub struct CuckooFilter {
    filter: ExternalFilter,
    capacity: i64,
    bucket_size: usize,
}

impl CuckooFilter {
    pub fn new(capacity: i64, bucket_size: usize, max_kicks: u32) -> Self {
        let filter = ExternalFilter::with_config_and_rng(
            capacity as usize,
            bucket_size,
            max_kicks,
            ChaCha8Rng::seed_from_u64(RNG_SEED),
        )
        .expect("validated filter configuration");
        let result = Self {
            filter,
            capacity,
            bucket_size,
        };
        result.cuckoo_filter_incr_metrics_on_new_create();
        result
    }

    fn from_snapshot(
        snapshot: FilterSnapshot,
        bucket_size: usize,
        max_kicks: u32,
    ) -> Result<Self, CuckooError> {
        let mut rng = ChaCha8Rng::seed_from_u64(RNG_SEED);
        rng.set_word_pos(snapshot.rng_word_pos);
        let filter = ExternalFilter::from_export_with_rng(
            ExportedCuckooFilter {
                values: snapshot.values,
                length: snapshot.length,
            },
            bucket_size,
            max_kicks,
            rng,
        )
        .map_err(|_| CuckooError::DecodeFilterFailed)?;
        let result = Self {
            filter,
            capacity: snapshot.capacity,
            bucket_size,
        };
        result.cuckoo_filter_incr_metrics_on_new_create();
        Ok(result)
    }

    fn snapshot(&self) -> FilterSnapshot {
        let exported = self.filter.export();
        FilterSnapshot {
            capacity: self.capacity,
            values: exported.values,
            length: exported.length,
            rng_word_pos: self.filter.rng().get_word_pos(),
        }
    }

    pub fn add(&mut self, item: &[u8]) -> Result<bool, CuckooError> {
        if self.contains(item) {
            return Ok(false);
        }
        if self.num_items() >= self.capacity {
            return Err(CuckooError::FilterFull);
        }
        self.filter
            .try_add(item)
            .map_err(|_| CuckooError::FilterFull)?;
        crate::metrics::CUCKOO_NUM_ITEMS_ACROSS_OBJECTS.fetch_add(1, Ordering::Relaxed);
        Ok(true)
    }
    pub fn contains(&self, item: &[u8]) -> bool {
        self.filter.contains(item)
    }
    pub fn delete(&mut self, item: &[u8]) -> Result<bool, CuckooError> {
        let deleted = self.filter.delete(item);
        if deleted {
            crate::metrics::CUCKOO_NUM_ITEMS_ACROSS_OBJECTS.fetch_sub(1, Ordering::Relaxed);
        }
        Ok(deleted)
    }
    pub fn count(&self, item: &[u8]) -> u32 {
        u32::from(self.contains(item))
    }
    pub fn number_of_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.filter.memory_usage()
            - std::mem::size_of::<ExternalFilter>()
    }
    pub fn compute_size(capacity: i64, bucket_size: usize) -> usize {
        ExternalFilter::allocation_size(capacity as usize, bucket_size)
            .and_then(|n| {
                n.checked_add(std::mem::size_of::<Self>())
                    .ok_or(cuckoofilter::CuckooError::InvalidConfiguration)
            })
            .unwrap_or(usize::MAX)
    }
    pub fn create_copy_from(from: &Self) -> Self {
        let result = Self {
            filter: from.filter.clone(),
            capacity: from.capacity,
            bucket_size: from.bucket_size,
        };
        result.cuckoo_filter_incr_metrics_on_new_create();
        result
    }
    pub fn capacity(&self) -> i64 {
        self.capacity
    }
    pub fn num_items(&self) -> i64 {
        self.filter.len() as i64
    }
    pub fn bucket_size(&self) -> usize {
        self.bucket_size
    }
    pub fn bucket_count(&self) -> usize {
        self.filter.bucket_count()
    }
    fn cuckoo_filter_incr_metrics_on_new_create(&self) {
        use crate::metrics;
        metrics::CUCKOO_NUM_FILTERS_ACROSS_OBJECTS.fetch_add(1, Ordering::Relaxed);
        metrics::CUCKOO_OBJECT_TOTAL_MEMORY_BYTES
            .fetch_add(self.number_of_bytes(), Ordering::Relaxed);
        metrics::CUCKOO_CAPACITY_ACROSS_OBJECTS.fetch_add(self.capacity as u64, Ordering::Relaxed);
        metrics::CUCKOO_NUM_ITEMS_ACROSS_OBJECTS
            .fetch_add(self.num_items() as u64, Ordering::Relaxed);
    }
}

impl Drop for CuckooFilter {
    fn drop(&mut self) {
        use crate::metrics;
        metrics::CUCKOO_NUM_FILTERS_ACROSS_OBJECTS.fetch_sub(1, Ordering::Relaxed);
        metrics::CUCKOO_OBJECT_TOTAL_MEMORY_BYTES
            .fetch_sub(self.number_of_bytes(), Ordering::Relaxed);
        metrics::CUCKOO_NUM_ITEMS_ACROSS_OBJECTS
            .fetch_sub(self.num_items() as u64, Ordering::Relaxed);
        metrics::CUCKOO_CAPACITY_ACROSS_OBJECTS.fetch_sub(self.capacity as u64, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_BUCKET_SIZE: usize = crate::configs::CUCKOO_BUCKET_SIZE_DEFAULT as usize;
    const DEFAULT_MAX_KICKS: u32 = crate::configs::CUCKOO_MAX_KICKS_DEFAULT as u32;

    #[test]
    fn test_cuckoo_filter_basic_operations() {
        let mut cf = CuckooFilter::new(1000, DEFAULT_BUCKET_SIZE, DEFAULT_MAX_KICKS);

        let item = b"test_item";
        assert!(cf.add(item).unwrap());
        assert_eq!(cf.num_items(), 1);

        assert!(cf.contains(item));

        // Duplicate inserts keep one fingerprint.
        assert!(!cf.add(item).unwrap());
        assert_eq!(cf.num_items(), 1); // fingerprint count unchanged

        assert!(cf.delete(item).unwrap());
        assert_eq!(cf.num_items(), 0);
        assert!(!cf.contains(item));

        assert!(!cf.delete(item).unwrap());
    }

    #[test]
    fn test_cuckoo_object_basic_operations() {
        let mut co =
            CuckooObject::new_reserved(1000, DEFAULT_BUCKET_SIZE, DEFAULT_MAX_KICKS, 0, false)
                .unwrap();

        let item = b"test_item";

        assert_eq!(co.add_item(item, false).unwrap(), 1);
        assert_eq!(co.num_items(), 1);
        assert!(co.item_exists(item));

        // Duplicate add: returns 1, fingerprint count unchanged
        assert_eq!(co.add_item(item, false).unwrap(), 1);
        assert_eq!(co.num_items(), 1);

        assert_eq!(co.delete_item(item).unwrap(), 1);
        assert_eq!(co.num_items(), 0);
        assert!(!co.item_exists(item));
    }

    #[test]
    fn test_cuckoo_object_capacity_and_memory() {
        let co = CuckooObject::new_reserved(1000, DEFAULT_BUCKET_SIZE, DEFAULT_MAX_KICKS, 2, false)
            .unwrap();

        assert_eq!(co.capacity(), 1000);
        assert_eq!(co.num_filters(), 1);
        assert!(co.memory_usage() > 0);
    }

    #[test]
    fn test_cuckoo_filter_count() {
        let mut cf = CuckooFilter::new(1000, DEFAULT_BUCKET_SIZE, DEFAULT_MAX_KICKS);

        let item = b"test_item";

        cf.add(item).unwrap();
        assert_eq!(cf.count(item), 1);

        cf.add(item).unwrap();
        assert_eq!(cf.count(item), 1);
    }

    #[test]
    fn test_cuckoo_object_create_copy() {
        let mut co =
            CuckooObject::new_reserved(1000, DEFAULT_BUCKET_SIZE, DEFAULT_MAX_KICKS, 0, false)
                .unwrap();

        let item = b"test_item";
        co.add_item(item, false).unwrap();

        let copy = CuckooObject::create_copy_from(&co);

        assert_eq!(copy.num_items(), co.num_items());
        assert_eq!(copy.capacity(), co.capacity());
        assert!(copy.item_exists(item));
    }

    #[test]
    fn test_bad_bucket_size() {
        let result = CuckooObject::new_reserved(1000, 0, DEFAULT_MAX_KICKS, 0, false);
        assert_eq!(result.err(), Some(CuckooError::BadBucketSize));

        let result = CuckooObject::new_reserved(1000, 256, DEFAULT_MAX_KICKS, 0, false);
        assert_eq!(result.err(), Some(CuckooError::BadBucketSize));
    }

    #[test]
    fn test_bad_capacity() {
        let result =
            CuckooObject::new_reserved(0, DEFAULT_BUCKET_SIZE, DEFAULT_MAX_KICKS, 0, false);
        assert_eq!(result.err(), Some(CuckooError::BadCapacity));

        let result =
            CuckooObject::new_reserved(-1, DEFAULT_BUCKET_SIZE, DEFAULT_MAX_KICKS, 0, false);
        assert_eq!(result.err(), Some(CuckooError::BadCapacity));
    }

    #[test]
    fn test_encode_decode() {
        let mut co =
            CuckooObject::new_reserved(1000, DEFAULT_BUCKET_SIZE, DEFAULT_MAX_KICKS, 2, false)
                .unwrap();

        let item = b"test_item";
        co.add_item(item, false).unwrap();

        let encoded = co.encode_object().unwrap();
        assert!(!encoded.is_empty());

        let decoded = CuckooObject::decode_object(&encoded, false).unwrap();

        assert_eq!(decoded.expansion(), co.expansion());
        assert_eq!(decoded.bucket_size(), co.bucket_size());
        assert_eq!(decoded.max_kicks(), co.max_kicks());
        assert_eq!(decoded.capacity(), co.capacity());
    }
    #[test]
    fn snapshot_and_copy_resume_identical_evictions() {
        let mut original = CuckooObject::new_reserved(32, 2, 20, 2, false).unwrap();
        for item in 0..70_u64 {
            original.add_item(&item.to_le_bytes(), false).unwrap();
        }
        let bytes = original.encode_object().unwrap();
        let mut restored = CuckooObject::decode_object(&bytes, false).unwrap();
        let mut copied = CuckooObject::create_copy_from(&original);
        assert_eq!(bytes, restored.encode_object().unwrap());
        assert_eq!(bytes, copied.encode_object().unwrap());
        for item in 70..300_u64 {
            let key = item.to_le_bytes();
            original.add_item(&key, false).unwrap();
            restored.add_item(&key, false).unwrap();
            copied.add_item(&key, false).unwrap();
            assert_eq!(
                original.encode_object().unwrap(),
                restored.encode_object().unwrap()
            );
            assert_eq!(
                original.encode_object().unwrap(),
                copied.encode_object().unwrap()
            );
        }
        assert!(original
            .filters
            .iter()
            .any(|f| f.filter.rng().get_word_pos() > 0));
    }

    #[test]
    fn duplicate_in_old_filter_is_removed_by_one_delete() {
        let mut object = CuckooObject::new_reserved(8, 2, 20, 2, false).unwrap();
        let key = b"original";
        object.add_item(key, false).unwrap();
        for item in 0..30_u64 {
            object.add_item(&item.to_le_bytes(), false).unwrap();
        }
        assert!(object.num_filters() > 1);
        let before = object.encode_object().unwrap();
        object.add_item(key, false).unwrap();
        assert_eq!(before, object.encode_object().unwrap());
        assert_eq!(object.delete_item(key).unwrap(), 1);
        assert!(!object.item_exists(key));
    }

    #[test]
    fn failed_insert_preserves_snapshot_and_existing_items() {
        let mut object = CuckooObject::new_reserved(16, 1, 1, 0, false).unwrap();
        let mut inserted = Vec::new();
        for item in 0..100_u64 {
            let key = item.to_le_bytes();
            let before = object.encode_object().unwrap();
            if object.add_item(&key, false).is_ok() {
                inserted.push(key);
            } else {
                assert_eq!(before, object.encode_object().unwrap());
            }
            for key in &inserted {
                assert!(object.item_exists(key));
            }
        }
    }

    #[test]
    fn reject_corrupt_snapshots() {
        let object = CuckooObject::new_reserved(32, 4, 20, 2, false).unwrap();
        let bytes = object.encode_object().unwrap();
        for end in 0..bytes.len() {
            assert!(CuckooObject::decode_object(&bytes[..end], false).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(CuckooObject::decode_object(&trailing, false).is_err());
        let mut snapshot: ObjectSnapshot = bincode::deserialize(&bytes[1..]).unwrap();
        snapshot.filters[0].length = 1;
        let mut invalid = vec![CUCKOO_OBJECT_VERSION];
        bincode::serialize_into(&mut invalid, &snapshot).unwrap();
        assert!(CuckooObject::decode_object(&invalid, false).is_err());
    }

    #[test]
    fn freed_capacity_is_reused_before_scaling() {
        let mut object = CuckooObject::new_reserved(16, 4, 20, 1, false).unwrap();
        for item in 0..32_u64 {
            object.add_item(&item.to_le_bytes(), false).unwrap();
        }
        let count = object.num_filters();
        let capacity = object.capacity();
        for item in 0..16_u64 {
            object.delete_item(&item.to_le_bytes()).unwrap();
        }
        for item in 100..108_u64 {
            object.add_item(&item.to_le_bytes(), false).unwrap();
        }
        assert_eq!(object.num_filters(), count);
        assert_eq!(object.capacity(), capacity);
    }
}
