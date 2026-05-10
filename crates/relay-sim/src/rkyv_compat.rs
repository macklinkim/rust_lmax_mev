//! Phase 4 P4-D per-crate rkyv-with adapters for the alloy-primitives
//! types embedded in `RelaySimulationOutcome` / `LocalBundleShape` /
//! `RelaySimRequest`.
//!
//! Same shape as the existing `crates/simulator/src/rkyv_compat.rs`
//! (P3-E) and `crates/risk/src/rkyv_compat.rs` (P3-D) adapters. Adapters
//! cannot be shared across crates because rkyv-with bindings are tied
//! to the crate that defines the deriving struct.
//!
//! Note on `Vec<Bytes>` (`RelaySimRequest::txs`): per the P4-D
//! execution note v0.4 §DP-D13 the v0.4 plan listed a
//! `VecBytesAsVecVecU8` adapter. The implementation lands the same
//! semantic via a different representation: `RelaySimRequest::txs`
//! is typed as `Vec<Vec<u8>>` directly. Reason: rkyv 0.8's
//! DeserializeWith for non-Copy multi-level wrappers (`Vec<Bytes>` →
//! `Vec<Vec<u8>>`) requires non-trivial trait plumbing; using
//! `Vec<Vec<u8>>` natively gives the same on-wire bytes with zero
//! adapter code. The eventual P4-E `eth_callBundle` HTTP client
//! converts to/from `alloy_primitives::Bytes` at its own boundary
//! (cost: one allocation per tx; the txs vector is small).

use alloy_primitives::{B256, U256};
use rkyv::{
    rancor::Fallible,
    with::{ArchiveWith, DeserializeWith, SerializeWith},
    Archive, Place, Serialize,
};

/// Adapter: `alloy_primitives::U256` <-> `[u8; 32]` (big-endian).
pub struct U256AsBytes;

impl ArchiveWith<U256> for U256AsBytes {
    type Archived = <[u8; 32] as Archive>::Archived;
    type Resolver = <[u8; 32] as Archive>::Resolver;

    fn resolve_with(field: &U256, resolver: Self::Resolver, out: Place<Self::Archived>) {
        let bytes: [u8; 32] = field.to_be_bytes();
        bytes.resolve(resolver, out);
    }
}

impl<S: Fallible + ?Sized> SerializeWith<U256, S> for U256AsBytes
where
    [u8; 32]: Serialize<S>,
{
    fn serialize_with(field: &U256, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        let bytes: [u8; 32] = field.to_be_bytes();
        bytes.serialize(serializer)
    }
}

impl<D: Fallible + ?Sized> DeserializeWith<<[u8; 32] as Archive>::Archived, U256, D>
    for U256AsBytes
{
    fn deserialize_with(
        field: &<[u8; 32] as Archive>::Archived,
        _deserializer: &mut D,
    ) -> Result<U256, D::Error> {
        let bytes: [u8; 32] = *field;
        Ok(U256::from_be_bytes(bytes))
    }
}

/// Adapter: `alloy_primitives::B256` <-> `[u8; 32]`.
pub struct B256AsBytes;

impl ArchiveWith<B256> for B256AsBytes {
    type Archived = <[u8; 32] as Archive>::Archived;
    type Resolver = <[u8; 32] as Archive>::Resolver;

    fn resolve_with(field: &B256, resolver: Self::Resolver, out: Place<Self::Archived>) {
        let bytes: [u8; 32] = field.0;
        bytes.resolve(resolver, out);
    }
}

impl<S: Fallible + ?Sized> SerializeWith<B256, S> for B256AsBytes
where
    [u8; 32]: Serialize<S>,
{
    fn serialize_with(field: &B256, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        let bytes: [u8; 32] = field.0;
        bytes.serialize(serializer)
    }
}

impl<D: Fallible + ?Sized> DeserializeWith<<[u8; 32] as Archive>::Archived, B256, D>
    for B256AsBytes
{
    fn deserialize_with(
        field: &<[u8; 32] as Archive>::Archived,
        _deserializer: &mut D,
    ) -> Result<B256, D::Error> {
        let bytes: [u8; 32] = *field;
        Ok(B256::from(bytes))
    }
}
