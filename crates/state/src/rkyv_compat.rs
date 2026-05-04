//! Phase 3 P3-A spec-compliance repair: per-crate rkyv-with adapters
//! for the alloy-primitives types embedded in `PoolId` / `PoolState` /
//! `StateUpdateEvent`. Same shape and rationale as
//! `crates/ingress/src/rkyv_compat.rs`; lives per-crate (not in
//! `crates/types`) per Codex direction (2026-05-04 15:10:16 +09:00).

use alloy_primitives::{Address, B256, U256};
use rkyv::{
    rancor::Fallible,
    with::{ArchiveWith, DeserializeWith, SerializeWith},
    Archive, Place, Serialize,
};

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

/// Adapter: `alloy_primitives::Address` <-> `[u8; 20]`.
pub struct AddressAsBytes;

impl ArchiveWith<Address> for AddressAsBytes {
    type Archived = <[u8; 20] as Archive>::Archived;
    type Resolver = <[u8; 20] as Archive>::Resolver;

    fn resolve_with(field: &Address, resolver: Self::Resolver, out: Place<Self::Archived>) {
        let bytes: [u8; 20] = field.0.into();
        bytes.resolve(resolver, out);
    }
}

impl<S: Fallible + ?Sized> SerializeWith<Address, S> for AddressAsBytes
where
    [u8; 20]: Serialize<S>,
{
    fn serialize_with(field: &Address, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        let bytes: [u8; 20] = field.0.into();
        bytes.serialize(serializer)
    }
}

impl<D: Fallible + ?Sized> DeserializeWith<<[u8; 20] as Archive>::Archived, Address, D>
    for AddressAsBytes
{
    fn deserialize_with(
        field: &<[u8; 20] as Archive>::Archived,
        _deserializer: &mut D,
    ) -> Result<Address, D::Error> {
        let bytes: [u8; 20] = *field;
        Ok(Address::from(bytes))
    }
}

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
