//! Phase 4 P4-C2 fixture-recording tool — operator-only.
//!
//! Records the V2 WETH/USDC pair, V3 0.05% WETH/USDC pool, WETH9, USDC
//! FiatTokenProxy, and FiatTokenV2_2 implementation account at a single
//! recently-finalized mainnet block hash, then prints them as Rust
//! literal text to stdout for paste-into `crates/simulator/src/fixtures.rs`.
//!
//! ## Usage
//!
//! ```bash
//! $env:ARCHIVE_RPC_URL = "https://eth-mainnet.g.alchemy.com/v2/<KEY>"
//! cargo run --release --example dump_fixture -- 0x<32-byte-block-hash>
//! ```
//!
//! ## Hardening invariants
//!
//! - `ARCHIVE_RPC_URL` is the ONLY input channel for the URL. Never
//!   read from a config file. **Never logged**: the URL value is moved
//!   into a `NodeConfig` and dropped; the printed output contains only
//!   the recorded slot values + bytecode + the literal block-hash arg.
//! - The `BLOCK_HASH_HEX` CLI arg IS echoed in stderr because it is
//!   public chain data and the operator needs to know which block was
//!   recorded.
//! - The tool is registered as `[[example]]`, NOT a test. CI never runs
//!   `cargo run --example`. No `#[test]` attribute. No
//!   `tokio::test` macro.
//! - No write to repo paths. Output goes to stdout only.
//! - No funded key. No `live_send`. No relay submission. No
//!   `eth_sendBundle`. Read-only `eth_getCode` + `eth_getStorageAt`.
//!
//! ## USDC slot-list workflow (iterative)
//!
//! The initial USDC proxy slot list below is conservative — it covers
//! the EIP-1967 implementation slot, three target `balanceOf` mappings,
//! and head slots `{0..16}` for the Ownable / Pausable / Blacklistable /
//! FiatTokenV1/V2/V2_2 inheritance chain head fields. The real
//! FiatTokenV2_2 inheritance read-set during `transfer` is NOT
//! independently documented here; if the planned `T-USDC-1` test in
//! P4-C2 reports `StrictMissingError::MissingStorage { slot }`, add
//! that slot here and re-record. Do NOT assume EIP-1967 layout is
//! sufficient — the actual storage-touch set is what matters.
//!
//! Note: this tool deliberately avoids hardcoding the USDC implementation
//! address. It is parsed from the proxy fixture's EIP-1967 slot value at
//! recording time so an upgrade between recordings is automatically
//! reflected.

use std::env;
use std::process::ExitCode;
use std::str::FromStr;

use alloy_primitives::{address, Address, B256, U256};
use rust_lmax_mev_config::{FallbackRpcConfig, NodeConfig};
use rust_lmax_mev_node::NodeProvider;
use rust_lmax_mev_state::{PoolId, PoolKind};
use rust_lmax_mev_state_fetcher::storage_key::{address_key, mapping_slot_u256};
use rust_lmax_mev_state_fetcher::uniswap::{UniswapV2Layout, UniswapV3Fee005Layout};
use rust_lmax_mev_state_fetcher::{
    ArchiveStateFetcher, FetchedAccount, FetchedPoolState, StateFetcher, StateFetcherConfig,
};

// --- Mainnet addresses (public chain data) -------------------------------
const WETH9: Address = address!("c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2");
const USDC_PROXY: Address = address!("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48");
const V2_WETH_USDC: Address = address!("b4e16d0168e52d35cacd2c6185b44281ec28c9dc");
const V3_005_WETH_USDC: Address = address!("88e6a0c2ddd26feeb64f039a2c41296fcb3f5640");

/// `MOCK_ROUTER_ADDRESS` placeholder used by P4-C2 LocalSimulator real-revm
/// path. Public test marker (not derived from a key); duplicated here so
/// the recorded WETH/USDC `balanceOf[router]` slots match what the
/// LocalSimulator inserts into `StrictMissingDb` at simulate time.
const MOCK_ROUTER: Address = address!("3333333333333333333333333333333333333333");

// --- ERC-20 mapping slot indices (per canonical Solidity layouts) --------
const WETH9_BALANCES_SLOT: u64 = 3;
/// FiatTokenV2_2 `balances` mapping. The slot index is per the published
/// FiatTokenV2_2 storage layout; `T-USDC-1` will catch a mismatch.
const USDC_BALANCES_SLOT: u64 = 9;
/// EIP-1967 implementation slot:
/// `bytes32(uint256(keccak256("eip1967.proxy.implementation")) - 1)
///  = 0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc`.
/// We read this slot to discover the implementation address rather than
/// hardcoding it.
const EIP1967_IMPL_SLOT_BE: [u8; 32] = [
    0x36, 0x08, 0x94, 0xa1, 0x3b, 0xa1, 0xa3, 0x21, 0x06, 0x67, 0xc8, 0x28, 0x49, 0x2d, 0xb9, 0x8d,
    0xca, 0x3e, 0x20, 0x76, 0xcc, 0x37, 0x35, 0xa9, 0x20, 0xa3, 0xca, 0x50, 0x5d, 0x38, 0x2b, 0xbc,
];

#[tokio::main]
async fn main() -> ExitCode {
    // 1. Read archive URL from env. NEVER printed.
    let archive_url = match env::var("ARCHIVE_RPC_URL") {
        Ok(u) if !u.is_empty() => u,
        _ => {
            eprintln!("error: ARCHIVE_RPC_URL env var unset or empty");
            eprintln!("       set it to your archive-mode RPC endpoint and re-run");
            eprintln!(
                "       (URL is never logged; only the recorded slot values + bytecode are emitted)"
            );
            return ExitCode::from(2);
        }
    };

    // 2. Read block hash from CLI.
    let Some(block_hash_hex) = env::args().nth(1) else {
        eprintln!("error: missing CLI arg <BLOCK_HASH_HEX>");
        eprintln!("usage: cargo run --release --example dump_fixture -- 0x<32-byte-hash>");
        return ExitCode::from(2);
    };
    let block_hash = match parse_block_hash(&block_hash_hex) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: bad BLOCK_HASH_HEX: {e}");
            return ExitCode::from(2);
        }
    };

    // 3. Build a minimal NodeConfig. Only `archive_rpc` is functional;
    //    `geth_ws_url` / `geth_http_url` / `fallback_rpc[0]` are required
    //    by the schema but never invoked by this tool (we only call
    //    archive methods). The same archive URL is reused across all
    //    three fields so URL parsing succeeds without leaking the value.
    let node_config = NodeConfig {
        geth_ws_url: "ws://127.0.0.1:65535/never-used".to_string(),
        geth_http_url: archive_url.clone(),
        fallback_rpc: vec![FallbackRpcConfig {
            url: archive_url.clone(),
            label: "archive-only-tool".to_string(),
        }],
        archive_rpc: Some(FallbackRpcConfig {
            url: archive_url,
            label: "archive".to_string(),
        }),
    };

    let provider = match NodeProvider::connect(&node_config).await {
        Ok(p) => std::sync::Arc::new(p),
        Err(e) => {
            eprintln!("error: NodeProvider::connect failed: {e}");
            return ExitCode::from(3);
        }
    };
    drop(node_config); // ensure URL string is dropped from this scope.

    let fetcher = ArchiveStateFetcher::new(provider, StateFetcherConfig::defaults());

    // 4. Header for the recording session (stderr; doesn't go into fixtures).
    eprintln!("# P4-C2 fixture recording");
    eprintln!("# block_hash: {block_hash_hex}");
    eprintln!("# (paste the stdout below into crates/simulator/src/fixtures.rs)");
    eprintln!();

    // stdout preamble.
    println!("// AUTO-GENERATED by `cargo run --release --example dump_fixture`.");
    println!("// Block hash: {block_hash_hex}");
    println!("// DO NOT edit by hand. Re-run dump_fixture if T-USDC-1 reports a");
    println!("// MissingStorage error for an unrecorded slot.");
    println!();

    // 5. V2 pool.
    let v2 = match fetcher
        .fetch_pool(&pool_id_v2(), block_hash, &UniswapV2Layout)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: V2 fetch failed: {e}");
            return ExitCode::from(4);
        }
    };
    print_pool_const("V2_WETH_USDC", &v2);

    // 6. V3 0.05% pool.
    let v3 = match fetcher
        .fetch_pool(&pool_id_v3_005(), block_hash, &UniswapV3Fee005Layout)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: V3 fetch failed: {e}");
            return ExitCode::from(4);
        }
    };
    print_pool_const("V3_WETH_USDC_005", &v3);

    // 7. WETH9 — code + balanceOf[router/pools].
    let weth_slots = [
        mapping_slot_u256(U256::from(WETH9_BALANCES_SLOT), address_key(MOCK_ROUTER)),
        mapping_slot_u256(U256::from(WETH9_BALANCES_SLOT), address_key(V2_WETH_USDC)),
        mapping_slot_u256(
            U256::from(WETH9_BALANCES_SLOT),
            address_key(V3_005_WETH_USDC),
        ),
    ];
    let weth = match fetcher.fetch_account(WETH9, &weth_slots, block_hash).await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: WETH9 fetch failed: {e}");
            return ExitCode::from(4);
        }
    };
    print_account_const("WETH9", &weth);

    // 8. USDC proxy. Conservative initial slot list — see module-doc
    //    "USDC slot-list workflow (iterative)" above. T-USDC-1 will
    //    surface any unrecorded slot via StrictMissingError::MissingStorage.
    //
    // Slots: EIP-1967 implementation address + balanceOf for the three
    // actors + head slots {0..16} for inheritance-chain modifier
    // preconditions (Ownable._owner, Pausable._paused, Blacklistable
    // .blacklister, FiatTokenV2_2 head fields). Not all are read by
    // transfer() but they're cheap to record and act as a safety net.
    let mut usdc_proxy_slots: Vec<U256> = vec![
        U256::from_be_bytes(EIP1967_IMPL_SLOT_BE),
        mapping_slot_u256(U256::from(USDC_BALANCES_SLOT), address_key(MOCK_ROUTER)),
        mapping_slot_u256(U256::from(USDC_BALANCES_SLOT), address_key(V2_WETH_USDC)),
        mapping_slot_u256(
            U256::from(USDC_BALANCES_SLOT),
            address_key(V3_005_WETH_USDC),
        ),
    ];
    usdc_proxy_slots.extend((0u64..16).map(U256::from));
    let usdc_proxy = match fetcher
        .fetch_account(USDC_PROXY, &usdc_proxy_slots, block_hash)
        .await
    {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: USDC proxy fetch failed: {e}");
            return ExitCode::from(4);
        }
    };
    print_account_const("USDC_PROXY", &usdc_proxy);

    // 9. USDC implementation. Address is parsed from the proxy fixture's
    //    EIP-1967 slot value (NOT hardcoded — an upgrade between
    //    recordings is automatically reflected).
    let impl_slot_value = match usdc_proxy
        .storage
        .iter()
        .find(|(s, _)| *s == U256::from_be_bytes(EIP1967_IMPL_SLOT_BE))
    {
        Some((_, v)) => *v,
        None => {
            eprintln!("error: USDC proxy fixture missing EIP-1967 implementation slot");
            return ExitCode::from(5);
        }
    };
    if impl_slot_value == B256::ZERO {
        eprintln!("error: USDC proxy EIP-1967 implementation slot is zero (not a proxy?)");
        return ExitCode::from(5);
    }
    let impl_addr = Address::from_slice(&impl_slot_value.0[12..32]);

    let usdc_impl = match fetcher.fetch_account(impl_addr, &[], block_hash).await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: USDC impl fetch failed: {e}");
            return ExitCode::from(4);
        }
    };
    print_account_const("USDC_IMPL", &usdc_impl);

    // 10. Footer (stderr; doesn't go into fixtures).
    eprintln!();
    eprintln!("done. Pasted constants live at crates/simulator/src/fixtures.rs.");
    eprintln!("If P4-C2 T-USDC-1 reports a MissingStorage error for an unrecorded");
    eprintln!("slot, add that slot to `usdc_proxy_slots` in dump_fixture.rs and");
    eprintln!("re-run with the same BLOCK_HASH_HEX.");
    ExitCode::SUCCESS
}

fn parse_block_hash(s: &str) -> Result<B256, String> {
    let stripped = s.strip_prefix("0x").unwrap_or(s);
    if stripped.len() != 64 {
        return Err(format!(
            "expected 32-byte hex (64 chars), got {} chars",
            stripped.len()
        ));
    }
    B256::from_str(s).map_err(|e| format!("hex parse: {e}"))
}

fn pool_id_v2() -> PoolId {
    PoolId {
        kind: PoolKind::UniswapV2,
        address: V2_WETH_USDC,
    }
}

fn pool_id_v3_005() -> PoolId {
    PoolId {
        kind: PoolKind::UniswapV3Fee005,
        address: V3_005_WETH_USDC,
    }
}

fn print_pool_const(name: &str, p: &FetchedPoolState) {
    println!("// --- {name} ({:?}) ---", p.pool.kind);
    println!(
        "pub const {}_ADDRESS: [u8; 20] = {};",
        name,
        addr_array_literal(&p.pool.address)
    );
    println!(
        "pub const {}_BLOCK_HASH: [u8; 32] = {};",
        name,
        b256_array_literal(&p.block_hash)
    );
    println!(
        "pub const {}_CODE: &[u8] = &{};",
        name,
        bytes_array_literal(&p.pool_code)
    );
    println!("pub const {}_STORAGE: &[(u64, [u8; 32])] = &[", name);
    for (slot, value) in &p.pool_storage {
        // Slot may exceed u64; format as full hex string in a comment +
        // emit the low u64 portion for the array literal. The fixture
        // consumer reads via `U256::from(low_u64)` for slot keys that
        // fit; mapping-derived slots are full U256 and the consumer
        // uses the u128/U256 form. Emit the slot as full 32-byte BE
        // for full fidelity.
        println!(
            "    // slot 0x{}",
            hex_encode_lower(&slot.to_be_bytes::<32>())
        );
        println!(
            "    ({}, {}),  // [WARN] slot truncated to u64 if it exceeds u64::MAX — see hex above",
            slot_low_u64_or_panic_comment(slot),
            b256_array_literal(value)
        );
    }
    println!("];");
    println!();
}

fn print_account_const(name: &str, a: &FetchedAccount) {
    println!("// --- {name} ---");
    println!(
        "pub const {}_ADDRESS: [u8; 20] = {};",
        name,
        addr_array_literal(&a.address)
    );
    println!(
        "pub const {}_BLOCK_HASH: [u8; 32] = {};",
        name,
        b256_array_literal(&a.block_hash)
    );
    println!(
        "pub const {}_CODE: &[u8] = &{};",
        name,
        bytes_array_literal(&a.code)
    );
    println!("pub const {}_STORAGE: &[([u8; 32], [u8; 32])] = &[", name);
    for (slot, value) in &a.storage {
        println!(
            "    ({}, {}),",
            b256_array_literal_from_u256(slot),
            b256_array_literal(value)
        );
    }
    println!("];");
    println!();
}

fn addr_array_literal(a: &Address) -> String {
    let mut out = String::from("[");
    for (i, b) in a.0.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("0x{b:02x}"));
    }
    out.push(']');
    out
}

fn b256_array_literal(b: &B256) -> String {
    let mut out = String::from("[");
    for (i, byte) in b.0.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("0x{byte:02x}"));
    }
    out.push(']');
    out
}

fn b256_array_literal_from_u256(u: &U256) -> String {
    let mut out = String::from("[");
    for (i, byte) in u.to_be_bytes::<32>().iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("0x{byte:02x}"));
    }
    out.push(']');
    out
}

fn bytes_array_literal(bytes: &[u8]) -> String {
    let mut out = String::from("[");
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            if i % 16 == 0 {
                out.push_str(",\n    ");
            } else {
                out.push_str(", ");
            }
        }
        out.push_str(&format!("0x{b:02x}"));
    }
    out.push(']');
    out
}

fn hex_encode_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn slot_low_u64_or_panic_comment(slot: &U256) -> String {
    // Cheap-cast to u64; mapping-derived slots will overflow this and
    // print as `0` — that's fine because we already emit the full
    // 32-byte hex in the comment line above. Fixtures consumers (P4-C2
    // commits) will switch to the `[u8; 32]` form for slot keys when
    // they can't be represented as u64.
    let raw = slot.to_be_bytes::<32>();
    if raw[..24].iter().all(|b| *b == 0) {
        let mut low = [0u8; 8];
        low.copy_from_slice(&raw[24..32]);
        u64::from_be_bytes(low).to_string()
    } else {
        "0 /* slot exceeds u64; use [u8;32] form via comment hex above */".to_string()
    }
}
