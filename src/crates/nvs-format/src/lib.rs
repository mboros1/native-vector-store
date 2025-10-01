use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: &str = "nvs.v1";
pub const ENDIANNESS_LITTLE: &str = "little";

// Magic headers (ASCII) + version byte
pub const META_IDX_MAGIC: &[u8; 8] = b"NVSIDX\0\x01";   // 8 bytes
pub const META_BLOCKS_MAGIC: &[u8; 8] = b"NVSMETA\x01"; // 8 bytes

#[repr(C)]
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct MetaIdxEntry {
    pub block_id: u32,
    pub offset_in_block: u32,
    pub doc_size: u32,
    pub reserved0: u32, // zero today; reserved for future use
}

pub const META_IDX_ENTRY_SIZE: usize = core::mem::size_of::<MetaIdxEntry>();

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MetaBlockHeader {
    pub comp_size: u32,
    pub decomp_size: u32,
    pub doc_count: u32,
    pub codec: u32, // 0=none, 1=zstd
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DType {
    F16,
    F32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct VectorLayout {
    pub rows: u64,
    pub cols: u64,
    pub row_alignment: u32, // bytes, e.g. 64
    pub dtype: DType,
}

#[inline]
pub fn row_stride_bytes(cols: usize, dtype: DType, align: usize) -> usize {
    let elem = match dtype { DType::F16 => 2, DType::F32 => 4 };
    let row = cols * elem;
    ((row + align - 1) / align) * align
}
