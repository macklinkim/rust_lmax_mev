//! Phase 3 P3-A spec-compliance repair: `rkyv::with::ArchiveWith` /
//! `SerializeWith` / `DeserializeWith` adapters for the alloy-primitives
//! types embedded in `MempoolEvent` and `BlockEvent`.
//!
//! Workspace `alloy-primitives = "0.8"` is pinned without an `rkyv` feature,
//! so the alloy newtypes (`B256`, `Address`, `U256`, `Bytes`) carry no
//! built-in `Archive`/`Serialize`/`Deserialize` impls. Each adapter below
//! converts the runtime alloy type to its canonical byte representation
//! for the archived form (fixed-size arrays for the primitives, `Vec<u8>`
//! for the variable-length `Bytes`), then reconstructs the alloy type on
//! deserialization. Public API of `MempoolEvent` / `BlockEvent` is
//! unchanged (DP-A per the approved P3-A execution note).

use alloy_primitives::{Address, Bytes, B256, U256};
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

/// Adapter: `alloy_primitives::Bytes` <-> `Vec<u8>`.
pub struct BytesAsVec;

impl ArchiveWith<Bytes> for BytesAsVec {
    type Archived = <Vec<u8> as Archive>::Archived;
    type Resolver = <Vec<u8> as Archive>::Resolver;

    fn resolve_with(field: &Bytes, resolver: Self::Resolver, out: Place<Self::Archived>) {
        let v: Vec<u8> = field.to_vec();
        v.resolve(resolver, out);
    }
}

impl<S: Fallible + ?Sized> SerializeWith<Bytes, S> for BytesAsVec
where
    Vec<u8>: Serialize<S>,
{
    fn serialize_with(field: &Bytes, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        let v: Vec<u8> = field.to_vec();
        v.serialize(serializer)
    }
}

impl<D: Fallible + ?Sized> DeserializeWith<<Vec<u8> as Archive>::Archived, Bytes, D> for BytesAsVec
where
    <Vec<u8> as Archive>::Archived: rkyv::Deserialize<Vec<u8>, D>,
{
    fn deserialize_with(
        field: &<Vec<u8> as Archive>::Archived,
        deserializer: &mut D,
    ) -> Result<Bytes, D::Error> {
        let v: Vec<u8> = rkyv::Deserialize::<Vec<u8>, D>::deserialize(field, deserializer)?;
        Ok(Bytes::from(v))
    }
}
