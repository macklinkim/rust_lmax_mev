//! Phase 3 P3-F per-crate rkyv-with adapter for the alloy-primitives
//! types embedded in `BundleCandidate`. Same shape as P3-A/C/D/E.

use alloy_primitives::U256;
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
