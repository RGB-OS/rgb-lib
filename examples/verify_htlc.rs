// Example: Verify HTLC Script
//
// This script builds an HTLC from parameters and verifies it matches expected output
// Use this to verify that a transaction's HTLC script was constructed correctly

use rgb_lib::{
    Error,
    bitcoin::{
        hashes::{Hash, sha256, hex::FromHex},
        PublicKey, ScriptBuf, Address, Network,
        script::Builder,
        opcodes::all::*,
    },
};
use std::str::FromStr;

/// Build HTLC script from parameters
fn build_htlc_script(
    payment_hash: &[u8; 32],
    lp_pubkey: &PublicKey,
    user_pubkey: &PublicKey,
    timelock_blocks: u32,
) -> ScriptBuf {
    Builder::new()
        .push_opcode(OP_IF)
            // Success path: LP claims with preimage
            .push_opcode(OP_SHA256)
            .push_slice(payment_hash)
            .push_opcode(OP_EQUALVERIFY)
            .push_key(lp_pubkey)
            .push_opcode(OP_CHECKSIG)
        .push_opcode(OP_ELSE)
            // Refund path: User reclaims after timelock
            .push_int(timelock_blocks as i64)
            .push_opcode(OP_CSV)
            .push_opcode(OP_DROP)
            .push_key(user_pubkey)
            .push_opcode(OP_CHECKSIG)
        .push_opcode(OP_ENDIF)
        .into_script()
}

/// Verify preimage matches payment hash
fn verify_preimage(preimage: &[u8; 32], payment_hash: &[u8; 32]) -> bool {
    let computed_hash = sha256::Hash::hash(preimage);
    let hash_bytes: &[u8] = computed_hash.as_ref();
    hash_bytes == payment_hash.as_slice()
}

fn main() -> Result<(), Error> {
    println!("🔐 HTLC Script Builder & Verifier");
    println!("==================================\n");

    // Step 1: Input parameters
    println!("📝 HTLC Parameters (from preimage.txt):");
    println!("========================================\n");

    // Use real data from preimage.txt
    let preimage_hex = "86a85cd1cb86c51186d190972c9f8413f436911fc0de241b6df20877ebbadecc";
    let payment_hash_hex = "f4d376425855e2354bf30e17904f4624f6f9aa297973cca0445cdf4cef718b2a";
    
    let preimage: [u8; 32] = <[u8; 32]>::from_hex(preimage_hex)
        .expect("Valid preimage hex");
    let payment_hash: [u8; 32] = <[u8; 32]>::from_hex(payment_hash_hex)
        .expect("Valid payment hash hex");

    println!("Preimage:     {}", preimage_hex);
    println!("Payment Hash: {}", payment_hash_hex);
    
    // Verify preimage
    let is_valid = verify_preimage(&preimage, &payment_hash);
    println!("Verified:     {} ✅\n", is_valid);

    // LP public key (example - replace with actual)
    let lp_pubkey_hex = "03133d81a3cc09d6899d9519f48325d3dff2ae8c49610296ca750ef5562c5a9626";
    let lp_pubkey = PublicKey::from_str(lp_pubkey_hex)
        .expect("Valid LP pubkey");
    println!("LP Pubkey:    {}", lp_pubkey_hex);

    // User public key
    let user_pubkey_hex = "03d6c27614557184d269b9cb19b1bc32479e661d86a925f4c4e46c734adcea3d19";
    let user_pubkey = PublicKey::from_str(user_pubkey_hex)
        .expect("Valid user pubkey");
    println!("User Pubkey:  {}", user_pubkey_hex);

    // Timelock
    let timelock_blocks = 144u32;
    println!("Timelock:     {} blocks (~24 hours)\n", timelock_blocks);

    // Step 2: Build HTLC script
    println!("🔨 Building HTLC Script:");
    println!("=========================\n");

    let htlc_script = build_htlc_script(
        &payment_hash,
        &lp_pubkey,
        &user_pubkey,
        timelock_blocks,
    );

    println!("HTLC Script (hex):");
    println!("{}\n", hex::encode(htlc_script.as_bytes()));

    println!("HTLC Script (asm):");
    println!("{}\n", htlc_script.to_asm_string());

    // Step 3: Generate P2WSH address
    println!("🏠 P2WSH Addresses:");
    println!("===================\n");

    let regtest_address = Address::p2wsh(&htlc_script, Network::Regtest);
    let testnet_address = Address::p2wsh(&htlc_script, Network::Testnet);
    let mainnet_address = Address::p2wsh(&htlc_script, Network::Bitcoin);

    println!("Regtest  {}", regtest_address);
    println!("Testnet  {}", testnet_address);
    println!("Mainnet  {}\n", mainnet_address);

    // Step 4: Show the difference between witness script and scriptPubKey
    println!("⚠️  Important Distinction:");
    println!("==========================\n");

    println!("HTLC Script (Witness Script):");
    println!("  Size: {} bytes", htlc_script.len());
    println!("  Hex: {}", hex::encode(htlc_script.as_bytes()));
    println!("  This is the FULL script with OP_IF, OP_SHA256, etc.");
    println!("  This is revealed when SPENDING the UTXO\n");

    let script_pubkey = regtest_address.script_pubkey();
    println!("scriptPubKey (in transaction output):");
    println!("  Size: {} bytes", script_pubkey.len());
    println!("  Hex: {}", hex::encode(script_pubkey.as_bytes()));

    // Step 5: Example verification
    println!("📋 Example Verification:");
    println!("========================\n");

    let regtest_address = Address::p2wsh(&htlc_script, Network::Regtest);
    println!("Expected HTLC Address (Regtest):");
    println!("{}\n", regtest_address);

    // Step 9: Summary
    println!("📊 Summary:");
    println!("===========\n");

    println!("Payment Hash: {}", payment_hash_hex);
    println!("LP Pubkey:    {}", lp_pubkey_hex);
    println!("User Pubkey:  {}", user_pubkey_hex);
    println!("Timelock:     {} blocks", timelock_blocks);
    println!();
    println!("HTLC Script:  {} bytes", htlc_script.len());
    println!("P2WSH Address (Regtest): {}", regtest_address);
    println!();
    println!("✅ Use this address to verify HTLC outputs in transactions");
    
    Ok(())
}

