//! Frame encode/decode helpers and file-header constants.
//!
//! Gate 3 scaffold placeholder. The `pub(crate)` constants (`MAGIC`,
//! `FILE_FORMAT_VERSION`, `RESERVED_HEADER`, `FILE_HEADER_LEN`,
//! `MAX_FRAME_LEN`, `FRAME_OVERHEAD`) and the helpers (`write_file_header`,
//! `read_and_validate_file_header`, `validate_frame_length`) land during
//! Gate 5 implementation per spec sections B.1.1 and B.1.4. The CRC32 path
//! uses the IEEE polynomial via `crc32fast` per spec section X.9.
