// Example: User Payer - Send RGB Assets to HTLC Invoice
//
// This example demonstrates the USER side of the atomic swap:
// 1. Create RGB wallet
// 2. Fund wallet with BTC
// 3. Create UTXOs
// 4. Issue RGB asset
// 5. Send RGB asset to HTLC invoice (with witness_data!)
// 6. Monitor transfer status

use rgb_lib::{
    wallet::{Wallet, WalletData, Online, DatabaseType, Recipient, WitnessData},
    Error, BitcoinNetwork, AssetSchema, Assignment,
    keys::generate_keys,
};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use std::thread;
use serde_json::json;

/// Bitcoin regtest RPC helper
struct RegtestRpc {
    base_url: String,
}

impl RegtestRpc {
    fn new(base_url: String) -> Self {
        Self { base_url }
    }

    /// Send BTC to an address
    fn send_to_address(&self, address: &str, amount: f64) -> Result<String, Error> {
        let client = reqwest::blocking::Client::new();
        
        println!("   💸 Sending {} BTC to {}...", amount, address);
        
        let response = client
            .post(&format!("{}/execute", self.base_url))
            .header("Content-Type", "application/json")
            .json(&json!({
                "args": format!("sendtoaddress {} {}", address, amount)
            }))
            .send()
            .map_err(|e| Error::Internal {
                details: format!("Failed to send BTC: {}", e),
            })?;

        if !response.status().is_success() {
            let error = response.text().unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::Internal {
                details: format!("RPC error: {}", error),
            });
        }

        let result: serde_json::Value = response.json()
            .map_err(|e| Error::Internal {
                details: format!("Failed to parse response: {}", e),
            })?;

        println!("   ✅ Transaction sent: {}", result);
        Ok(result.to_string())
    }

    /// Mine blocks
    fn mine(&self, blocks: u32) -> Result<(), Error> {
        let client = reqwest::blocking::Client::new();
        
        println!("   ⛏️  Mining {} blocks...", blocks);
        
        let response = client
            .post(&format!("{}/execute", self.base_url))
            .header("Content-Type", "application/json")
            .json(&json!({
                "args": format!("mine {}", blocks)
            }))
            .send()
            .map_err(|e| Error::Internal {
                details: format!("Failed to mine: {}", e),
            })?;

        if !response.status().is_success() {
            let error = response.text().unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::Internal {
                details: format!("RPC error: {}", error),
            });
        }

        println!("   ✅ Mined {} blocks", blocks);
        Ok(())
    }
}

fn main() -> Result<(), Error> {
    println!("🧑 RGB Asset Payer - Atomic Swap User Side");
    println!("==========================================\n");

    // Initialize Bitcoin RPC client
    let rpc = RegtestRpc::new("http://18.119.98.232:5000".to_string());

    // Step 1: Create user wallet
    println!("📝 Step 1: Creating User Wallet");
    println!("================================\n");

    let data_dir = std::env::temp_dir().join("atomic_swap_user");
    if !data_dir.exists() {
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| Error::Internal { details: format!("Failed to create dir: {}", e) })?;
    }
    println!("📁 Data directory: {:?}\n", data_dir);

    println!("🔑 Generating user wallet keys...");
    let user_keys = generate_keys(BitcoinNetwork::Regtest);
    println!("   ✅ Mnemonic: {}", user_keys.mnemonic);
    println!("   ✅ Master fingerprint: {}\n", user_keys.master_fingerprint);

    let wallet_data = WalletData {
        data_dir: data_dir.to_string_lossy().to_string(),
        bitcoin_network: BitcoinNetwork::Regtest,
        database_type: DatabaseType::Sqlite,
        max_allocations_per_utxo: 1,
        account_xpub_vanilla: user_keys.account_xpub_vanilla.clone(),
        account_xpub_colored: user_keys.account_xpub_colored.clone(),
        mnemonic: Some(user_keys.mnemonic.clone()),
        master_fingerprint: user_keys.master_fingerprint.clone(),
        vanilla_keychain: Some(1),
        supported_schemas: vec![
            AssetSchema::Nia,
            AssetSchema::Cfa,
        ],
    };

    let mut wallet = Wallet::new(wallet_data)?;
    println!("   ✅ User wallet created!\n");

    // Step 2: Go online
    println!("📝 Step 2: Bringing Wallet Online");
    println!("==================================\n");

    #[cfg(any(feature = "electrum", feature = "esplora"))]
    let online = {
        println!("🌐 Connecting to Electrum...");
        let online = wallet.go_online(
            false,
            "tcp://regtest.thunderstack.org:50001".to_string(),
        )?;
        println!("   ✅ Wallet is online!\n");
        online
    };

    #[cfg(not(any(feature = "electrum", feature = "esplora")))]
    {
        println!("⚠️  Electrum feature not enabled. Compile with --features electrum\n");
        return Ok(());
    }

    // Step 3: Fund wallet
    println!("📝 Step 3: Funding Wallet with BTC");
    println!("===================================\n");

    let address = wallet.get_address()?;
    println!("🏠 Wallet address: {}\n", address);

    println!("💰 Requesting 0.1 BTC from regtest faucet...");
    rpc.send_to_address(&address, 0.1)?;
    println!();

    println!("⛏️  Mining blocks to confirm...");
    rpc.mine(1)?;
    println!();

    // Sync wallet
    println!("🔄 Syncing wallet...");
    wallet.sync(online.clone())?;
    
    let balance = wallet.get_btc_balance(Some(online.clone()), false)?;
    println!("   ✅ BTC Balance: {} sats (settled), {} sats (future)\n", 
             balance.vanilla.settled, balance.vanilla.future);

    // Step 4: Create UTXOs
    println!("📝 Step 4: Creating UTXOs");
    println!("==========================\n");

    println!("🔨 Creating 5 UTXOs...");
    let created_utxos = wallet.create_utxos(
        online.clone(),
        false,      // up_to
        Some(5),    // num
        None,       // size
        2,          // fee_rate (u64, in sat/vB)
        false,      // skip_sync
    )?;
    println!("   ✅ Created {} UTXOs", created_utxos);
    println!();

    println!("⛏️  Mining to confirm UTXOs...");
    rpc.mine(1)?;
    println!();

    println!("🔄 Syncing wallet...");
    wallet.sync(online.clone())?;
    println!("   ✅ UTXOs confirmed\n");

    // Step 5: Issue RGB asset
    println!("📝 Step 5: Issuing RGB Asset");
    println!("=============================\n");

    println!("🎨 Issuing NIA asset (100 units)...");
    let asset = wallet.issue_asset_nia(
        "ATOMIC".to_string(),     // ticker
        "Atomic Swap Token".to_string(), // name
        0,                         // precision 0 (no decimals, simple integers)
        vec![100],                 // amounts (100 whole tokens)
    )?;

    println!("   ✅ Asset issued!");
    println!("   Asset ID: {}", asset.asset_id);
    println!();

    println!("⛏️  Mining to confirm issuance...");
    rpc.mine(1)?;
    println!();

    println!("🔄 Syncing wallet...");
    wallet.sync(online.clone())?;
    
    let asset_balance = wallet.get_asset_balance(asset.asset_id.clone())?;
    println!("   ✅ Asset balance: {} units (settled), {} units (future)\n", 
             asset_balance.settled, asset_balance.future);

    // Step 6: Get HTLC invoice from LP
    println!("📝 Step 6: Getting HTLC Invoice");
    println!("================================\n");

    println!("📧 Enter the RGB invoice from LP (or press Enter for example):");
    println!("   Format: rgb:~/~/XabF/bcrt:wvout:...\n");

    use std::io::{self, Write};
    print!("Invoice: ");
    io::stdout().flush().unwrap();
    
    let mut invoice_string = String::new();
    io::stdin().read_line(&mut invoice_string).unwrap();
    invoice_string = invoice_string.trim().to_string();

    if invoice_string.is_empty() {
        println!("⚠️  No invoice provided. Using example format.");
        return Ok(());
    }

    // Parse invoice to get recipient_id
    println!("🔍 Parsing invoice...");
    let invoice_data = rgb_lib::wallet::Invoice::new(invoice_string.clone())?
        .invoice_data();
    
    println!("   ✅ Invoice parsed!");
    println!("   Recipient ID: {}", invoice_data.recipient_id);
    println!("   Assignment: {:?}", invoice_data.assignment);
    println!();

    // Determine if this is a witness recipient
    let is_witness = invoice_data.recipient_id.contains("wvout:");
    println!("   📋 Recipient type: {}", if is_witness { "Witness" } else { "Blinded" });

    if !is_witness {
        println!("   ⚠️  This is not a witness recipient. No witness_data needed.");
    } else {
        println!("   ⚠️  This IS a witness recipient. witness_data REQUIRED!");
    }
    println!();

    // Step 7: Send RGB asset to HTLC invoice
    println!("📝 Step 7: Sending RGB Asset to HTLC");
    println!("=====================================\n");

    // Get the amount from invoice or use default
    let amount = match invoice_data.assignment {
        Assignment::Fungible(amt) => amt,
        _ => 13, // Default amount
    };

    println!("📤 Preparing to send {} units of {}...", amount, asset.asset_id);
    
    // 🎯 KEY: Provide witness_data for witness recipients!
    let recipient = Recipient {
        recipient_id: invoice_data.recipient_id.clone(),
        assignment: Assignment::Fungible(amount),
        witness_data: if is_witness {
            println!("   ✅ Adding witness_data (amount_sat: 1000, blinding: None)");
            Some(WitnessData {
                amount_sat: 1000,  // Min UTXO amount
                blinding: None,    // Random blinding
            })
        } else {
            None
        },
        transport_endpoints: invoice_data.transport_endpoints.clone(),
    };
    println!();

    let recipient_map = HashMap::from([(
        asset.asset_id.clone(),
        vec![recipient],
    )]);

    println!("🚀 Sending RGB asset...");
    let send_result = wallet.send(
        online.clone(),
        recipient_map,
        true,  // donation
        2,      // fee_rate (u64, in sat/vB)
        1,      // min_confirmations
        false,  // skip_sync
    )?;
    
    println!("   ✅ Send complete!");
    println!("   Transaction ID: {}\n", send_result.txid);

    // Step 8: Mine and monitor
    println!("📝 Step 8: Mining and Monitoring Transfer");
    println!("==========================================\n");

    println!("⛏️  Mining 1 block to confirm transaction...");
    rpc.mine(1)?;
    println!();

    println!("🔄 Starting transfer monitoring...");
    println!("   (Will check every 10 seconds, press Ctrl+C to stop)\n");

    let start_time = Instant::now();
    let mut check_count = 0;

    loop {
        check_count += 1;
        let elapsed = start_time.elapsed();
        
        println!("🔍 Check #{} ({}s elapsed)...", check_count, elapsed.as_secs());
        
        // Refresh wallet to sync transfer status
        println!("   🔄 Refreshing wallet...");
        let refresh_result = wallet.refresh(
            online.clone(),
            Some(asset.asset_id.clone()),
            vec![],
            false,
        )?;
        
        println!("   📊 Refresh complete: {} transfers updated", refresh_result.len());

        // Get all transfers
        let transfers = wallet.list_transfers(Some(asset.asset_id.clone()))?;
        
        // Find our transfer
        let mut found_settled = false;
        for transfer in transfers {
            if transfer.recipient_id == Some(invoice_data.recipient_id.clone()) {
                println!("   ✅ Found transfer to HTLC!");
                println!("      Status: {:?}", transfer.status);
                println!("      Recipient: {}", transfer.recipient_id.as_ref().unwrap());
                
                use rgb_lib::TransferStatus;
                match transfer.status {
                    TransferStatus::Settled => {
                        println!("\n🎉 SUCCESS! Transfer is SETTLED!");
                        println!("   ✅ RGB assets sent to HTLC address");
                        println!("   ✅ LP can now detect the funding");
                        println!("   ✅ Atomic swap can proceed!\n");
                        // found_settled = true;
                        break;
                    }
                    TransferStatus::WaitingCounterparty => {
                        println!("      ⏳ Status: WaitingCounterparty");
                        println!("         (Waiting for LP to accept consignment)");
                    }
                    TransferStatus::WaitingConfirmations => {
                        println!("      ⏳ Status: WaitingConfirmations");
                        println!("         (Waiting for more block confirmations)");
                    }
                    _ => {
                        println!("      ⏳ Status: {:?}", transfer.status);
                    }
                }
                break;
            }
        }

        if found_settled {
            break;
        }

        // Mine more blocks if needed
        if check_count % 3 == 0 {
            println!("   ⛏️  Mining additional block to help confirmations...");
            rpc.mine(1)?;
        }

        println!("   💤 Sleeping 10 seconds before next check...\n");
        thread::sleep(Duration::from_secs(10));
    }

    println!("📊 Monitoring Summary:");
    println!("   Total checks: {}", check_count);
    println!("   Duration: {} seconds", start_time.elapsed().as_secs());
    println!("   Asset sent: {} units of {}", amount, asset.asset_id);
    println!();

    println!("✨ User payer complete! Wallet saved at: {:?}", data_dir);
    Ok(())
}

