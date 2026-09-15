use crate::cuckoo::utils::{CuckooObject, CUCKOO_OBJECT_VERSION};
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
        let data = raw::load_string_buffer(rdb).ok()?;
        match Self::decode_object(data.as_ref(), true) {
            Ok(object) => Some(object),
            Err(err) => {
                logging::log_warning(err.as_str());
                None
            }
        }
    }

    fn debug_digest(&self, mut dig: Digest) {
        // Include every fingerprint and the RNG position, not just item counts.
        if let Ok(bytes) = self.encode_object() {
            dig.add_string_buffer(&bytes);
        }
        dig.end_sequence();
    }
}

/// Save a complete snapshot including the RNG stream position.
///
/// # Safety
/// `rdb` must be a valid Valkey persistence context.
pub unsafe fn rdb_save_cuckoo_object(rdb: *mut raw::RedisModuleIO, value: &CuckooObject) {
    match value.encode_object() {
        Ok(data) => {
            raw::RedisModule_SaveStringBuffer.unwrap()(rdb, data.as_ptr().cast(), data.len())
        }
        Err(err) => logging::log_warning(err.as_str()),
    }
}

pub fn cuckoo_rdb_aux_load(_rdb: *mut raw::RedisModuleIO) -> c_int {
    raw::Status::Ok as i32
}
