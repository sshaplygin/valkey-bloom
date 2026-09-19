use crate::cuckoo::utils::{CuckooFilter, CuckooObject, CUCKOO_OBJECT_VERSION};
use crate::wrapper::cuckoo_callback;
use std::os::raw::c_int;
use valkey_module::digest::Digest;
use valkey_module::native_types::ValkeyType;
use valkey_module::{logging, raw};

const CUCKOO_TYPE_ENCODING_VERSION: i32 = CUCKOO_OBJECT_VERSION as i32;

pub static CUCKOO_TYPE: ValkeyType = ValkeyType::new(
    "cuckooflt",
    CUCKOO_TYPE_ENCODING_VERSION,
    raw::RedisModuleTypeMethods {
        version: raw::REDISMODULE_TYPE_METHOD_VERSION as u64,
        rdb_load: Some(cuckoo_callback::cuckoo_rdb_load),
        rdb_save: Some(cuckoo_callback::cuckoo_rdb_save),
        aof_rewrite: Some(cuckoo_callback::cuckoo_aof_rewrite),
        digest: Some(cuckoo_callback::cuckoo_digest),

        mem_usage: Some(cuckoo_callback::cuckoo_mem_usage),
        free: Some(cuckoo_callback::cuckoo_free),

        aux_load: Some(cuckoo_callback::cuckoo_aux_load),
        // Callback not needed as there is no AUX (out of keyspace) data to be saved.
        aux_save: None,
        aux_save2: None,
        aux_save_triggers: raw::Aux::Before as i32,

        free_effort: Some(cuckoo_callback::cuckoo_free_effort),
        // Callback not needed as it just notifies us when a cuckoo item is about to be freed.
        unlink: None,
        copy: Some(cuckoo_callback::cuckoo_copy),
        defrag: Some(cuckoo_callback::cuckoo_defrag),

        // The callbacks below are not needed since the version 1 variants are used when implemented.
        mem_usage2: None,
        free_effort2: None,
        unlink2: None,
        copy2: None,
    },
);

pub trait ValkeyDataType {
    fn load_from_rdb(rdb: *mut raw::RedisModuleIO, encver: i32) -> Option<CuckooObject>;
    fn debug_digest(&self, dig: Digest);
}

impl ValkeyDataType for CuckooObject {
    fn load_from_rdb(rdb: *mut raw::RedisModuleIO, encver: i32) -> Option<CuckooObject> {
        if encver != CUCKOO_TYPE_ENCODING_VERSION {
            logging::log_warning("Unsupported cuckoo persistence version.");
            return None;
        }
        fn load_header<const N: usize>(rdb: *mut raw::RedisModuleIO) -> Option<[u64; N]> {
            let mut fields = [0; N];
            for field in &mut fields {
                *field = raw::load_unsigned(rdb).ok()?;
            }
            Some(fields)
        }
        let header @ [expansion, bucket_size, max_kicks, count] = load_header(rdb)?;
        CuckooObject::validate_snapshot_header(header).ok()?;
        let mut filters = Vec::with_capacity(1);
        for _ in 0..count {
            let header = load_header(rdb)?;
            let size = CuckooFilter::validate_snapshot_header(header, bucket_size as usize).ok()?;
            let data = raw::load_string_buffer(rdb).ok()?;
            if data.as_ref().len() != size {
                return None;
            }
            let filter = CuckooFilter::from_snapshot(
                header,
                data.as_ref().into(),
                bucket_size as usize,
                max_kicks as u32,
            )
            .ok()?;
            filters.push(Box::new(filter));
        }
        let object = Self::from_existing(
            expansion as u32,
            bucket_size as usize,
            max_kicks as u32,
            filters,
        );
        if !Self::validate_size(object.memory_usage()) {
            logging::log_warning(format!(
                "Loaded cuckoo object using {} bytes, exceeding local memory limit {}.",
                object.memory_usage(),
                crate::configs::CUCKOO_MEMORY_LIMIT_PER_OBJECT
                    .load(std::sync::atomic::Ordering::Relaxed)
            ));
        }
        Some(object)
    }

    fn debug_digest(&self, mut dig: Digest) {
        for field in self.snapshot_header() {
            dig.add_long_long(field as i64);
        }
        for filter in self.filters() {
            for field in filter.snapshot_header() {
                dig.add_long_long(field as i64);
            }
            dig.add_string_buffer(filter.as_bytes());
        }
        dig.end_sequence();
    }
}

/// Save directly from the bucket allocations without creating a snapshot buffer.
///
/// # Safety
/// `rdb` must be a valid Valkey persistence context.
pub unsafe fn rdb_save_cuckoo_object(rdb: *mut raw::RedisModuleIO, value: &CuckooObject) {
    for field in value.snapshot_header() {
        raw::RedisModule_SaveUnsigned.unwrap()(rdb, field);
    }
    for filter in value.filters() {
        for field in filter.snapshot_header() {
            raw::RedisModule_SaveUnsigned.unwrap()(rdb, field);
        }
        let bytes = filter.as_bytes();
        raw::RedisModule_SaveStringBuffer.unwrap()(rdb, bytes.as_ptr().cast(), bytes.len());
    }
}

pub fn cuckoo_rdb_aux_load(_rdb: *mut raw::RedisModuleIO) -> c_int {
    raw::Status::Ok as i32
}
