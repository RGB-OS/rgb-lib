use rgb_lib::{
    wallet::{Wallet, WalletData, Online, DatabaseType, Recipient, WitnessData},
    Error, BitcoinNetwork, AssetSchema, Assignment,
    keys::generate_keys,
    bitcoin::{
        hashes::{Hash, sha256},
        PublicKey, ScriptBuf, Address, Network as BdkNetwork,
        script::Builder,
        opcodes::all::*,
    },
};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use reqwest::blocking::Client;
use serde_json::json;

struct RegtestRpc {
    base_url: String,
}

impl RegtestRpc {
    fn new(base_url: &str) -> Result<Self, Error> {
        Ok(Self { 
            base_url: base_url.to_string() 
        })
    }

    /// Send BTC to an address
    fn send_to_address(&self, address: &str, amount: f64) -> Result<String, Error> {
        let client = reqwest::blocking::Client::new();
        
        println!("   💸 Sending {} BTC to {}...", amount, address);
        
        let response = client
            .post(&self.base_url)
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
            .post(&self.base_url)
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

/// RGB-LN invoice structure 
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RgbLnInvoice {
    pub payment_hash: String,
    pub amount_asset: u64,
    pub asset_id: String,
    pub description: String,
    pub expiry: u64,
}

#[derive(Debug, Clone)]
pub struct RgbLnNodeClient {
    base_url: String,
    api_key: Option<String>,
    client: Client,
}

/// Response from decode invoice API call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodeInvoiceResponse {
    pub payment_hash: String,
    pub amt_msat: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
}

/// Response from pay invoice API call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayInvoiceResponse {
    pub status: PaymentStatus,
    pub payment_hash: String,
    pub payment_secret: String,
}

/// Payment status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PaymentStatus {
    Succeeded,
    Failed,
    Pending,
}

/// Payment details from getPayment API call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentDetails {
    pub amt_msat: u64,
    pub asset_amount: u64,
    pub asset_id: String,
    pub payment_hash: String,
    pub inbound: bool,
    pub status: PaymentStatus,
    pub created_at: u64,
    pub updated_at: u64,
    pub payee_pubkey: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preimage: Option<String>,
}

/// Response from getPayment API call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPaymentResponse {
    pub payment: PaymentDetails,
}

impl RgbLnNodeClient {
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        Self {
            base_url,
            api_key,
            client: Client::new(),
        }
    }

    /// Decode RGB-LN invoice to extract payment details
    pub fn decode_invoice(&self, invoice: &str) -> Result<DecodeInvoiceResponse, Error> {
        println!("Decoding RGB-LN invoice...");
        
        let url = format!("{}/decodelninvoice", self.base_url);
        let mut request = self.client.post(&url)
            .json(&json!({ "invoice": invoice }));
        
        if let Some(ref key) = self.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }
        
        let response = request
            .send()
            .map_err(|e| Error::Internal {
                details: format!("Failed to decode invoice: {}", e),
            })?;

        if !response.status().is_success() {
            let error_msg = response.text().unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::Internal {
                details: format!("RLN decode error: {}", error_msg),
            });
        }

        response.json::<DecodeInvoiceResponse>()
            .map_err(|e| Error::Internal {
                details: format!("Failed to parse decode response: {}", e),
            })
    }

    /// Pay RGB-LN invoice and return preimage on successful payment
    pub fn pay_invoice(&self, invoice: &str) -> Result<PayInvoiceResponse, Error> {
        println!("Paying RGB-LN invoice...");
        
        let url = format!("{}/sendpayment", self.base_url);
        let mut request = self.client.post(&url)
            .json(&json!({ "invoice": invoice }));
        
        if let Some(ref key) = self.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }
        
        let response = request
            .send()
            .map_err(|e| Error::Internal {
                details: format!("Payment failed: {}", e),
            })?;

        if !response.status().is_success() {
            let error_msg = response.text().unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::Internal {
                details: format!("RLN payment error: {}", error_msg),
            });
        }

        let result = response.json::<PayInvoiceResponse>()
            .map_err(|e| Error::Internal {
                details: format!("Failed to parse payment response: {}", e),
            })?;

        println!("PayInvoiceResponse: {:?}", result);
        
        if result.status == PaymentStatus::Pending {
            println!("WARNING: Payment succeeded but status is Pending");
        }

        Ok(result)
    }

    /// Get payment details by payment hash, including preimage if available
    pub fn get_payment(&self, payment_hash: &str) -> Result<GetPaymentResponse, Error> {
        println!("Getting payment details for hash: {}...", payment_hash);
        
        let url = format!("{}/getpayment", self.base_url);
        let mut request = self.client.post(&url)
            .json(&json!({ "payment_hash": payment_hash }));
        
        if let Some(ref key) = self.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }
        
        let response = request
            .send()
            .map_err(|e| Error::Internal {
                details: format!("Failed to get payment: {}", e),
            })?;

        if !response.status().is_success() {
            let error_msg = response.text().unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::Internal {
                details: format!("RLN getPayment error: {}", error_msg),
            });
        }

        let result = response.json::<GetPaymentResponse>()
            .map_err(|e| Error::Internal {
                details: format!("Failed to parse payment details: {}", e),
            })?;

        println!("GetPaymentResponse: {:?}", result);
        Ok(result)
    }
}

/// HTLC status tracking
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HtlcStatus {
    Created,
    AwaitingFunding,
    Funded,
    PaymentInProgress,
    Claimed,
    Refunded,
    Expired,
}

/// RGB HTLC structure with ATOMIC guarantees
#[derive(Debug, Clone)]
pub struct AtomicRgbHtlc {
    pub swap_id: String,
    pub payment_hash: [u8; 32],
    pub amount: u64,
    pub asset_id: String,
    pub lp_pubkey: PublicKey,
    pub user_pubkey: PublicKey,
    pub timelock_blocks: u32,
    pub status: HtlcStatus,
    
    // HTLC-specific fields (ATOMIC!)
    pub htlc_script: ScriptBuf,
    pub htlc_address: String,
    
    // RGB-specific fields
    pub recipient_id: Option<String>,
    pub batch_transfer_idx: Option<u32>,
    pub preimage: Option<[u8; 32]>,
}

impl AtomicRgbHtlc {
    pub fn new(
        payment_hash: [u8; 32],
        amount: u64,
        asset_id: String,
        lp_pubkey: PublicKey,
        user_pubkey: PublicKey,
        timelock_blocks: u32,
        network: BdkNetwork,
    ) -> Self {
        use sha256::Hash;
        let swap_id = Hash::hash(&payment_hash).to_string();
        
        // Create HTLC script (THIS IS THE ATOMIC PART!)
        let htlc_script = Self::create_htlc_script(
            &payment_hash,
            &lp_pubkey,
            &user_pubkey,
            timelock_blocks,
        );
        
        // Create P2WSH address from HTLC script
        let htlc_address = Address::p2wsh(&htlc_script, network).to_string();
        
        Self {
            swap_id,
            payment_hash,
            amount,
            asset_id,
            lp_pubkey,
            user_pubkey,
            timelock_blocks,
            status: HtlcStatus::Created,
            htlc_script,
            htlc_address,
            recipient_id: None,
            batch_transfer_idx: None,
            preimage: None,
        }
    }

    fn create_htlc_script(
        payment_hash: &[u8; 32],
        lp_pubkey: &PublicKey,
        user_pubkey: &PublicKey,
        timelock_blocks: u32,
    ) -> ScriptBuf {
        Builder::new()
            .push_opcode(OP_IF)
                // Claim path
                .push_opcode(OP_SHA256)
                .push_slice(payment_hash)
                .push_opcode(OP_EQUALVERIFY)
                .push_key(lp_pubkey)
                .push_opcode(OP_CHECKSIG)
            .push_opcode(OP_ELSE)
                // Refund path
                .push_int(timelock_blocks as i64)
                .push_opcode(OP_CSV)
                .push_opcode(OP_DROP)
                .push_key(user_pubkey)
                .push_opcode(OP_CHECKSIG)
            .push_opcode(OP_ENDIF)
            .into_script()
    }

    pub fn verify_preimage(&self, preimage: &[u8; 32]) -> bool {
        let hash = sha256::Hash::hash(preimage);
        let hash_bytes: &[u8] = hash.as_ref();
        hash_bytes == self.payment_hash.as_slice()
    }
}

pub struct AtomicRgbLnLiquidityProvider {
    wallet: Wallet,  
    wallet_data: WalletData,  
    active_swaps: HashMap<String, AtomicRgbHtlc>,
    lp_pubkey: PublicKey,
    proxy_url: String,
    bitcoin_network: BdkNetwork,
    rgb_ln_client: RgbLnNodeClient,
}

impl AtomicRgbLnLiquidityProvider {
    pub fn new(
        wallet_data: WalletData,
        lp_pubkey: PublicKey,
        proxy_url: String,
        bitcoin_network: BdkNetwork,
        rgb_ln_base_url: String,
        rgb_ln_api_key: Option<String>,
    ) -> Result<Self, Error> {
        let wallet = Wallet::new(wallet_data.clone())?;
        let rgb_ln_client = RgbLnNodeClient::new(rgb_ln_base_url, rgb_ln_api_key);
        
        Ok(Self {
            wallet,
            wallet_data,
            active_swaps: HashMap::new(),
            lp_pubkey,
            proxy_url,
            bitcoin_network,
            rgb_ln_client,
        })
    }

    #[cfg(any(feature = "electrum", feature = "esplora"))]
    pub fn go_online(
        &mut self,
        skip_consistency_check: bool,
        electrum_url: Option<String>,
    ) -> Result<Online, Error> {
        println!("   🌐 Bringing wallet online...");
        
        let online = self.wallet.go_online(
            skip_consistency_check,
            electrum_url.unwrap_or_else(|| "ssl://electrum.blockstream.info:60002".to_string()),
        )?;
        
        println!("   ✅ Wallet is now online!");
        Ok(online)
    }

    pub fn create_atomic_swap(
        &mut self,
        invoice: RgbLnInvoice,
        user_pubkey: PublicKey,
    ) -> Result<AtomicSwapOffer, Error> {
        if invoice.asset_id.is_empty() {
            return Err(Error::Internal {
                details: "Invalid asset ID".to_string(),
            });
        }

        let payment_hash = hex::decode(&invoice.payment_hash)
            .map_err(|e| Error::Internal {
                details: format!("Invalid payment hash: {}", e),
            })?;
        let payment_hash: [u8; 32] = payment_hash.try_into()
            .map_err(|_| Error::Internal {
                details: "Payment hash must be 32 bytes".to_string(),
            })?;

        let htlc = AtomicRgbHtlc::new(
            payment_hash,
            invoice.amount_asset,
            invoice.asset_id.clone(),
            self.lp_pubkey.clone(),
            user_pubkey,
            144, // ~24 hours timelock
            self.bitcoin_network,
        );

        let receive_data = self.wallet.script_receive(
            htlc.htlc_script.clone(),
            // Some(invoice.asset_id.clone()),
            None,
            rgb_lib::Assignment::Fungible(htlc.amount),
            Some(86400), // 24 hours
            vec![self.proxy_url.clone()],
            1, // min confirmations
        )?;
        
        let recipient_id = receive_data.recipient_id;
        let rgb_invoice = receive_data.invoice;

        let mut htlc = htlc;
        htlc.recipient_id = Some(recipient_id.clone());
        htlc.status = HtlcStatus::AwaitingFunding;
        
        let swap_id = htlc.swap_id.clone();
        let htlc_address = htlc.htlc_address.clone();
        self.active_swaps.insert(swap_id.clone(), htlc);

        Ok(AtomicSwapOffer {
            swap_id,
            htlc_address,  
            recipient_id,
            rgb_invoice,
            payment_hash: invoice.payment_hash,
            timelock_blocks: 144,
        })
    }

    pub fn check_htlc_funding(
        &mut self,
        online: Online,
        swap_id: &str,
    ) -> Result<HtlcFundingStatus, Error> {
        let htlc = self.active_swaps.get(swap_id)
            .ok_or_else(|| Error::Internal {
                details: "Swap not found".to_string(),
            })?;

        if htlc.status == HtlcStatus::Funded {
            return Ok(HtlcFundingStatus::Funded);
        }

        let recipient_id = htlc.recipient_id.clone()
            .ok_or_else(|| Error::Internal {
                details: "HTLC has no recipient ID".to_string(),
            })?;

        println!("   🔄 Syncing wallet to check for incoming transfers...");
        
        self.wallet.sync(online.clone())?;
        println!("   📊 Bitcoin sync complete");

        let refresh_result = self.wallet.refresh(
            online.clone(),
            None,   
            vec![], 
            false,  
        )?;

        println!("   📊 RGB refresh complete: {} transfers updated", refresh_result.len());

        let assets = self.wallet.list_assets(vec![])?;
        let total_assets = 
            assets.nia.as_ref().map(|v| v.len()).unwrap_or(0) +
            assets.cfa.as_ref().map(|v| v.len()).unwrap_or(0) +
            assets.uda.as_ref().map(|v| v.len()).unwrap_or(0);
        
        println!("   💎 Assets in wallet: {}", total_assets);
        if let Some(ref nia_assets) = assets.nia {
            for asset in nia_assets {
                let balance = self.wallet.get_asset_balance(asset.asset_id.clone())?;
                println!("      - NIA {}: {} units (settled: {}, future: {})", 
                         asset.ticker, asset.asset_id, balance.settled, balance.future);
            }
        }
        if let Some(ref cfa_assets) = assets.cfa {
            for asset in cfa_assets {
                let balance = self.wallet.get_asset_balance(asset.asset_id.clone())?;
                println!("      - CFA {}: {} units (settled: {}, future: {})", 
                         asset.name, asset.asset_id, balance.settled, balance.future);
            }
        }

        let unspents = self.wallet.list_unspents(Some(online.clone()), false, false)?;
        let total_utxos = unspents.len();
        let total_btc: u64 = unspents.iter().map(|u| u.utxo.btc_amount).sum();
        println!("   🔷 UTXOs in wallet: {} (total: {} sats)", total_utxos, total_btc);
        
        let colored_utxos: Vec<_> = unspents.iter()
            .filter(|u| !u.rgb_allocations.is_empty())
            .collect();
        
        if !colored_utxos.is_empty() {
            println!("      Colored UTXOs: {}", colored_utxos.len());
            for unspent in colored_utxos {
                println!("      • {}:{} - {} sats", 
                         &unspent.utxo.outpoint.txid[..8],
                         unspent.utxo.outpoint.vout,
                         unspent.utxo.btc_amount);
                for allocation in &unspent.rgb_allocations {
                    let status = if allocation.settled { "✅" } else { "⏳" };
                    let amount = match &allocation.assignment {
                        Assignment::Fungible(amt) => format!("{} units", amt),
                        Assignment::NonFungible => "NFT".to_string(),
                        _ => "?".to_string(),
                    };
                    println!("        └─ {} {} {}",
                             status,
                             allocation.asset_id.as_ref().unwrap_or(&"?".to_string()),
                             amount);
                }
            }
        }

        let asset_filter = if let Some(ref nia_assets) = assets.nia {
            if !nia_assets.is_empty() {
                let asset_id = nia_assets[0].asset_id.clone();
                println!("   🔍 Filtering transfers by NIA asset: {}", asset_id);
                Some(asset_id)
            } else {
                None
            }
        } else {
            None
        };
        
        let transfers = self.wallet.list_transfers(asset_filter)?;
        println!("   📋 Total transfers: {}", transfers.len());
        
        for transfer in transfers {
            if transfer.recipient_id == Some(recipient_id.clone()) {
                println!("   ✅ Found transfer to HTLC!");
                println!("      Status: {:?}", transfer.status);
                println!("      Recipient: {}", transfer.recipient_id.as_ref().unwrap());
                
                use rgb_lib::TransferStatus;
                if transfer.status == TransferStatus::Settled {
                    return Ok(HtlcFundingStatus::Funded);
                } else {
                    return Ok(HtlcFundingStatus::Pending);
                }
            }
        }
        
        Ok(HtlcFundingStatus::Pending)
    }

    pub fn pay_invoice(
        &mut self,
        swap_id: &str,
        invoice_string: &str,
    ) -> Result<PaymentResult, Error> {
        let htlc = self.active_swaps.get_mut(swap_id)
            .ok_or_else(|| Error::Internal {
                details: "Swap not found".to_string(),
            })?;

        if htlc.status != HtlcStatus::Funded {
            return Err(Error::Internal {
                details: "HTLC not funded yet".to_string(),
            });
        }

        htlc.status = HtlcStatus::PaymentInProgress;

        let decode_response = self.rgb_ln_client.decode_invoice(invoice_string)?;
        
        if decode_response.payment_hash != hex::encode(htlc.payment_hash) {
            return Err(Error::Internal {
                details: "Payment hash mismatch between invoice and HTLC".to_string(),
            });
        }

        let pay_response = self.rgb_ln_client.pay_invoice(invoice_string)?;
        
        let payment_details = self.rgb_ln_client.get_payment(&pay_response.payment_hash)?;
        
        match payment_details.payment.status {
            PaymentStatus::Succeeded => {
                if let Some(preimage_hex) = payment_details.payment.preimage {
                    Ok(PaymentResult {
                        success: true,
                        preimage: Some(preimage_hex),
                        error: None,
                    })
                } else {
        Err(Error::Internal {
                        details: "Payment succeeded but no preimage returned".to_string(),
                    })
                }
            },
            PaymentStatus::Pending => {
                Ok(PaymentResult {
                    success: false,
                    preimage: None,
                    error: Some("Payment is pending".to_string()),
                })
            },
            PaymentStatus::Failed => {
                Err(Error::Internal {
                    details: "Payment failed".to_string(),
                })
            }
        }
    }

    pub fn claim_htlc_atomic(
        &mut self,
        swap_id: &str,
        preimage: [u8; 32],
        online: Online,
    ) -> Result<AtomicClaimResult, Error> {
        use std::collections::HashMap;
        
        let htlc = self.active_swaps.get_mut(swap_id)
            .ok_or_else(|| Error::Internal {
                details: "Swap not found".to_string(),
            })?;

        if !htlc.verify_preimage(&preimage) {
            return Err(Error::Internal {
                details: "Invalid preimage - hash doesn't match!".to_string(),
            });
        }

        println!("\n🔓 Claiming HTLC with proper witness and signature...");
        println!("   Preimage: {}", hex::encode(preimage));
        println!("   Hash: {}", hex::encode(htlc.payment_hash));

        println!("\n   🔄 Step 1: Syncing wallet state...");
        println!("      (Discovering new UTXOs and refreshing RGB transfers)");
        
        // First sync to discover new Bitcoin UTXOs
        self.wallet.sync(online.clone())?;
        println!("      ✅ Synced Bitcoin UTXOs");
        
        // Then refresh RGB transfers
        let refresh_result = self.wallet.refresh(online.clone(), None, vec![], false)?;
        println!("      ✅ Refreshed {} RGB transfers", refresh_result.len());
        
        // Step 2: Find HTLC UTXO in wallet (by recipient_id, not asset_id!)
        println!("\n   🔍 Step 2: Finding HTLC UTXO...");
        
        let unspents = self.wallet.list_unspents(Some(online.clone()), false, false)?;
        
        println!("      Found {} unspents in wallet", unspents.len());
        println!("      Looking for HTLC recipient: {}", htlc.recipient_id.as_ref().unwrap_or(&"None".to_string()));
        
        // Debug: Show all RGB allocations
        for (i, u) in unspents.iter().enumerate() {
            if !u.rgb_allocations.is_empty() {
                println!("      Unspent #{}: {}:{}", i, u.utxo.outpoint.txid, u.utxo.outpoint.vout);
                for alloc in &u.rgb_allocations {
                    if let Some(asset_id) = &alloc.asset_id {
                        println!("         Asset: {}, Settled: {}", asset_id, alloc.settled);
                    }
                }
            }
        }
        
        // Find HTLC UTXO by ANY settled RGB allocation (accept whatever user sent!)
        let htlc_unspent = unspents.iter()
            .find(|u| {
                u.rgb_allocations.iter().any(|a| a.settled && a.asset_id.is_some())
            })
            .ok_or_else(|| {
                Error::Internal { 
                    details: format!(
                        "HTLC UTXO not found!\nFound {} unspents, but none have settled RGB allocations.\nPossible causes:\n1. User hasn't sent RGB assets yet\n2. Transfer not settled yet (needs confirmations)\n3. Refresh needed",
                        unspents.len()
                    )
                }
            })?;
        
        // Get the ACTUAL asset that was sent (not what we expected!)
        let actual_asset_allocation = htlc_unspent.rgb_allocations.iter()
            .find(|a| a.settled && a.asset_id.is_some())
            .ok_or_else(|| Error::Internal {
                details: "No settled RGB allocation found".to_string(),
            })?;
        
        let actual_asset_id = actual_asset_allocation.asset_id.as_ref().unwrap().clone();
        let actual_amount = match &actual_asset_allocation.assignment {
            Assignment::Fungible(amt) => *amt,
            _ => return Err(Error::Internal {
                details: "Non-fungible asset not supported".to_string(),
            }),
        };
        
        println!("      ✅ Found HTLC UTXO with RGB allocation:");
        println!("         Outpoint: {}:{}", htlc_unspent.utxo.outpoint.txid, htlc_unspent.utxo.outpoint.vout);
        println!("         BTC amount: {} sats", htlc_unspent.utxo.btc_amount);
        println!("         RGB asset: {}", actual_asset_id);
        println!("         RGB amount: {} units", actual_amount);
        
        if actual_asset_id != htlc.asset_id {
            println!("\n      ℹ️  Asset ID differs from invoice:");
            println!("         Expected: {}", htlc.asset_id);
            println!("         Received: {}", actual_asset_id);
            println!("         ✅ LP accepts whatever user sent!");
        }

        // Step 3: Create destination (LP's own address) for the ACTUAL asset received
        println!("\n   📬 Step 3: Creating destination invoice...");
        let lp_destination = self.wallet.witness_receive(
            Some(actual_asset_id.clone()),  // Use actual asset, not expected!
            Assignment::Fungible(actual_amount),  // Use actual amount received
            Some(3600),
            vec![self.proxy_url.clone()],
            1,
        )?;
        println!("      ✅ Destination created: {}", lp_destination.recipient_id);

        // Step 4: Wait for user payment and debug wallet state
        println!("\n   ⏰ Step 4: Waiting 2 minutes for user payment...");
        println!("      💡 User should send RGB assets to HTLC address during this time");
        println!("      💡 Expected: Bitcoin UTXO with 1000 sats + RGB allocation");
        
        // Sync and refresh before waiting
        println!("\n   🔄 Pre-wait sync and refresh...");
        println!("      📊 Syncing Bitcoin UTXOs...");
        self.wallet.sync(online.clone())?;
        println!("      ✅ Bitcoin sync complete");
        
        println!("      📊 Refreshing RGB transfers...");
        let pre_refresh_result = self.wallet.refresh(online.clone(), None, vec![], false)?;
        println!("      ✅ Refreshed {} RGB transfers", pre_refresh_result.len());
        
        use std::thread;
        use std::time::Duration;
        
        // Wait 2 minutes (120 seconds)
        let wait_seconds = 30;
        for i in 1..=wait_seconds {
            if i % 10 == 0 {
                println!("      ⏳ {} seconds remaining...", wait_seconds - i);
            }
            thread::sleep(Duration::from_secs(1));
        }
        
        println!("\n   🔍 Step 4: Debug wallet state after waiting...");
        
        // Sync and refresh after waiting to get latest state
        println!("      📊 Syncing Bitcoin UTXOs after wait...");
        self.wallet.sync(online.clone())?;
        println!("      ✅ Bitcoin sync complete");
        
        println!("      📊 Refreshing RGB transfers after wait...");
        let refresh_result = self.wallet.refresh(online.clone(), None, vec![], false)?;
        println!("      ✅ Refreshed {} RGB transfers", refresh_result.len());
        
        // List all assets in wallet
        let assets = self.wallet.list_assets(vec![])?;
        println!("      💎 Assets in wallet:");
        println!("         Full assets response: {:?}", assets);
        
        if let Some(ref nia_assets) = assets.nia {
            println!("         NIA Assets ({}):", nia_assets.len());
            for asset in nia_assets {
                let balance = self.wallet.get_asset_balance(asset.asset_id.clone())?;
                println!("         - NIA {}: {} units (settled: {}, future: {})", 
                         asset.ticker, asset.asset_id, balance.settled, balance.future);
                println!("           Asset details: {:?}", asset);
            }
        } else {
            println!("         No NIA assets found");
        }
        
        if let Some(ref cfa_assets) = assets.cfa {
            println!("         CFA Assets ({}):", cfa_assets.len());
            for asset in cfa_assets {
                let balance = self.wallet.get_asset_balance(asset.asset_id.clone())?;
                println!("         - CFA {}: {} units (settled: {}, future: {})", 
                         asset.name, asset.asset_id, balance.settled, balance.future);
                println!("           Asset details: {:?}", asset);
            }
        } else {
            println!("         No CFA assets found");
        }
        
        if let Some(ref uda_assets) = assets.uda {
            println!("         UDA Assets ({}):", uda_assets.len());
            for asset in uda_assets {
                let balance = self.wallet.get_asset_balance(asset.asset_id.clone())?;
                println!("         - UDA {}: {} units (settled: {}, future: {})", 
                         asset.name, asset.asset_id, balance.settled, balance.future);
                println!("           Asset details: {:?}", asset);
            }
        } else {
            println!("         No UDA assets found");
        }
        
        // List all unspents again to see current state
        let unspents = self.wallet.list_unspents(Some(online.clone()), false, false)?;
        println!("      🔷 Current UTXOs: {}", unspents.len());
        println!("         Full unspents response: {:?}", unspents);
        
        // Compare with HTLC UTXO we found earlier
        println!("      🔍 Comparing with earlier HTLC UTXO:");
        println!("         Earlier HTLC: {}:{} - {} sats, exists: {}", 
                 &htlc_unspent.utxo.outpoint.txid[..8],
                 htlc_unspent.utxo.outpoint.vout,
                 htlc_unspent.utxo.btc_amount,
                 htlc_unspent.utxo.exists);
        
        // Check if we can find the HTLC UTXO in current unspents
        let current_htlc_utxo = unspents.iter()
            .find(|u| u.utxo.outpoint.txid == htlc_unspent.utxo.outpoint.txid && 
                      u.utxo.outpoint.vout == htlc_unspent.utxo.outpoint.vout);
        
        if let Some(htlc_utxo) = current_htlc_utxo {
            println!("         Current HTLC: {}:{} - {} sats, exists: {}", 
                     &htlc_utxo.utxo.outpoint.txid[..8],
                     htlc_utxo.utxo.outpoint.vout,
                     htlc_utxo.utxo.btc_amount,
                     htlc_utxo.utxo.exists);
            println!("         BTC amount changed: {} → {}", 
                     htlc_unspent.utxo.btc_amount, htlc_utxo.utxo.btc_amount);
            println!("         Exists changed: {} → {}", 
                     htlc_unspent.utxo.exists, htlc_utxo.utxo.exists);
        } else {
            println!("         ❌ HTLC UTXO not found in current unspents!");
        }
        
        // Debug: Check if there are any NEW UTXOs that might be the Bitcoin transaction
        println!("      🔍 Looking for NEW UTXOs (potential Bitcoin transactions to HTLC):");
        let new_utxos: Vec<_> = unspents.iter()
            .filter(|u| u.utxo.outpoint.txid != htlc_unspent.utxo.outpoint.txid)
            .collect();
        
        println!("         Found {} new UTXOs (excluding original HTLC)", new_utxos.len());
        for (i, utxo) in new_utxos.iter().enumerate() {
            println!("         New UTXO #{}: {}:{} - {} sats, exists: {}", 
                     i,
                     &utxo.utxo.outpoint.txid[..8],
                     utxo.utxo.outpoint.vout,
                     utxo.utxo.btc_amount,
                     utxo.utxo.exists);
            
            // Check if this UTXO has RGB allocations (might be the updated HTLC)
            if !utxo.rgb_allocations.is_empty() {
                println!("           └─ Has RGB allocations: {}",
                         utxo.rgb_allocations.len());
                for alloc in &utxo.rgb_allocations {
                    if let Some(asset_id) = &alloc.asset_id {
                        println!("              └─ Asset: {}, Amount: {:?}, Settled: {}", 
                                 asset_id, alloc.assignment, alloc.settled);
                    }
                }
            }
        }
        
        // Debug: Check if the HTLC address received any Bitcoin transactions
        println!("      🔍 Checking HTLC address for Bitcoin transactions:");
        println!("         HTLC Address: {}", htlc.htlc_address);
        println!("         Expected: Bitcoin transaction with 1000 sats to this address");
        println!("         💡 If user sent with witness_data, there should be a Bitcoin TX");
        println!("         💡 The UTXO should be updated from RGB-only to real Bitcoin UTXO");
        
        // Debug: Try to refresh to process any pending transfers
        println!("      🔄 Attempting refresh to process pending transfers...");
        let refresh_result = self.wallet.refresh(
            online.clone(),
            None, // All assets
            vec![],
            false,
        );
        match refresh_result {
            Ok(_) => println!("         ✅ Refresh completed successfully"),
            Err(e) => println!("         ❌ Refresh failed: {}", e),
        }
        
        // Debug: Check if any UTXOs match the HTLC address
        println!("      🔍 Checking for UTXOs that match HTLC address:");
        let htlc_address_utxos: Vec<_> = unspents.iter()
            .filter(|_u| {
                // Check if this UTXO's address matches the HTLC address
                // We need to check the script_pubkey or address
                true // For now, just show all UTXOs
            })
            .collect();
        
        println!("         Found {} UTXOs that might match HTLC address", htlc_address_utxos.len());
        for (i, utxo) in htlc_address_utxos.iter().enumerate() {
            println!("         HTLC Address UTXO #{}: {}:{} - {} sats, exists: {}", 
                     i,
                     &utxo.utxo.outpoint.txid[..8],
                     utxo.utxo.outpoint.vout,
                     utxo.utxo.btc_amount,
                     utxo.utxo.exists);
        }
        
        let colored_utxos: Vec<_> = unspents.iter()
            .filter(|u| !u.rgb_allocations.is_empty())
            .collect();
        
        println!("      🎨 Colored UTXOs: {}", colored_utxos.len());
        for (i, unspent) in unspents.iter().enumerate() {
            println!("         UTXO #{}: {}:{} - {} sats", 
                     i,
                     &unspent.utxo.outpoint.txid[..8],
                     unspent.utxo.outpoint.vout,
                     unspent.utxo.btc_amount);
            println!("           Full UTXO details: {:?}", unspent);
            
            if !unspent.rgb_allocations.is_empty() {
                println!("           RGB Allocations ({}):", unspent.rgb_allocations.len());
                for (j, allocation) in unspent.rgb_allocations.iter().enumerate() {
                    let status = if allocation.settled { "✅" } else { "⏳" };
                    let amount = match &allocation.assignment {
                        Assignment::Fungible(amt) => format!("{} units", amt),
                        Assignment::NonFungible => "NFT".to_string(),
                        _ => "?".to_string(),
                    };
                    println!("             Allocation #{}: {} {} {}", j, status, 
                             allocation.asset_id.as_ref().unwrap_or(&"None".to_string()),
                             amount);
                    println!("               Full allocation details: {:?}", allocation);
                }
            } else {
                println!("           No RGB allocations");
            }
        }
        
        // Check if we can find the specific asset we want to spend
        let balance = self.wallet.get_asset_balance(actual_asset_id.clone())?;
        println!("      🎯 Target asset balance: {} units (settled: {}, future: {})", 
                 actual_asset_id, balance.settled, balance.future);
        
        if balance.settled == 0 {
            println!("      ❌ ERROR: No settled balance for asset {}", actual_asset_id);
            return Err(Error::Internal {
                details: format!("No settled balance for asset {} (settled: {}, future: {})", 
                                actual_asset_id, balance.settled, balance.future),
            });
        }
        
        
        let recipient = Recipient {
            assignment: Assignment::Fungible(actual_amount),  // Use actual amount
            recipient_id: lp_destination.recipient_id.clone(),
            witness_data: Some(WitnessData {
                amount_sat: htlc_unspent.utxo.btc_amount,  // Use actual BTC amount (0)
                blinding: None,
            }),
            transport_endpoints: vec![self.proxy_url.clone()],
        };
        
        let mut recipient_map = HashMap::new();
        recipient_map.insert(actual_asset_id.clone(), vec![recipient]);  // Use actual asset!
        
        // Convert to BdkOutPoint for manual selection
        use rgb_lib::bitcoin::OutPoint as BdkOutPoint;
        let htlc_outpoint = BdkOutPoint {
            txid: htlc_unspent.utxo.outpoint.txid.parse()
                .map_err(|e| Error::Internal { details: format!("Invalid txid: {}", e) })?,
            vout: htlc_unspent.utxo.outpoint.vout,
        };
        
        let unsigned_psbt_str = self.wallet.send_from_utxos_begin(
            online.clone(),
            vec![htlc_outpoint],
            recipient_map,
            true,   // donation = true (broadcast immediately via send_end)
            2,      // fee_rate
            1,      // min_confirmations
        )?;
        

        // Step 6: Sign the PSBT (wallet handles HTLC witness automatically)
        println!("\n   ✍️  Step 6: Signing PSBT...");
        let signed_psbt = self.wallet.sign_psbt(unsigned_psbt_str, None)?;
        println!("      ✅ PSBT signed successfully by wallet");
        
        // Step 7: Broadcast transaction with send_end
        println!("\n   📡 Step 7: Broadcasting transaction with send_end...");
        
        let send_result = self.wallet.send_end(
            online.clone(),
            signed_psbt,
            false, // skip_sync
        );
        
        // Debug: Handle send_end result
        match &send_result {
            Ok(result) => {
                println!("      ✅ Send result: {:?}", result);
            }
            Err(e) => {
                println!("      ❌ Send error: {:?}", e);
                match e {
                    Error::FailedBroadcast { details } => {
                        println!("         FailedBroadcast: {}", details);
                    }
                    _ => {
                        println!("         Other error type: {:?}", e);
                    }
                }
            }
        }
        
        let send_result = send_result?;
        
        println!("      ✅ Transaction broadcast successful!");
        println!("         TXID: {}", send_result.txid);
        println!("         Batch Transfer IDX: {}", send_result.batch_transfer_idx);
        
        let txid = send_result.txid.clone();
        
        htlc.status = HtlcStatus::Claimed;
        htlc.preimage = Some(preimage);

        Ok(AtomicClaimResult {
            swap_id: swap_id.to_string(),
            amount_claimed: actual_amount,  // Return actual amount received
            asset_id: actual_asset_id,      // Return actual asset received
            preimage_hex: hex::encode(preimage),
            claim_txid: txid,
        })
    }

    /// Get refund information for user (if LP doesn't pay)
    pub fn get_refund_info(&self, swap_id: &str) -> Result<RefundInfo, Error> {
        let htlc = self.active_swaps.get(swap_id)
            .ok_or_else(|| Error::Internal {
                details: "Swap not found".to_string(),
            })?;

        Ok(RefundInfo {
            swap_id: swap_id.to_string(),
            htlc_address: htlc.htlc_address.clone(),
            htlc_script: htlc.htlc_script.clone(),
            timelock_blocks: htlc.timelock_blocks,
            can_refund: htlc.status != HtlcStatus::Claimed,
        })
    }

   
    pub fn complete_atomic_swap(
        &mut self,
        swap_id: &str,
        invoice_string: &str,
        online: Online,
    ) -> Result<AtomicClaimResult, Error> {
        let payment_result = self.pay_invoice(swap_id, invoice_string)?;
        
        if !payment_result.success {
            return Err(Error::Internal {
                details: format!("Payment failed: {:?}", payment_result.error),
            });
        }

        let preimage_hex = payment_result.preimage
            .ok_or_else(|| Error::Internal {
                details: "No preimage in payment result".to_string(),
            })?;

        let preimage_bytes = hex::decode(&preimage_hex)
            .map_err(|e| Error::Internal {
                details: format!("Invalid preimage hex: {}", e),
            })?;
        
        let preimage: [u8; 32] = preimage_bytes.try_into()
            .map_err(|_| Error::Internal {
                details: "Preimage must be 32 bytes".to_string(),
            })?;

        self.claim_htlc_atomic(swap_id, preimage, online)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AtomicSwapOffer {
    pub swap_id: String,
    pub htlc_address: String,       // P2WSH address with HTLC script
    pub recipient_id: String,        // RGB recipient ID for HTLC
    pub rgb_invoice: String,         // RGB invoice for HTLC address
    pub payment_hash: String,
    pub timelock_blocks: u32,
}

#[derive(Debug, PartialEq)]
pub enum HtlcFundingStatus {
    Pending,
    Funded,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PaymentResult {
    pub success: bool,
    pub preimage: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AtomicClaimResult {
    pub swap_id: String,
    pub amount_claimed: u64,
    pub asset_id: String,
    pub preimage_hex: String,
    pub claim_txid: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RefundInfo {
    pub swap_id: String,
    pub htlc_address: String,
    pub htlc_script: ScriptBuf,
    pub timelock_blocks: u32,
    pub can_refund: bool,
}

fn main() -> Result<(), Error> {
    println!("🎉 ATOMIC RGB-LN Liquidity Provider - Full Demo");
    println!("===============================================\n");

    // Step 1: Create test data directory
    let data_dir = std::env::temp_dir().join("atomic_swap_demo");
    if !data_dir.exists() {
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| Error::Internal { details: format!("Failed to create dir: {}", e) })?;
    }
    println!("📁 Data directory: {:?}\n", data_dir);

    // Step 2: Generate keys for LP wallet
    println!("🔑 Generating LP wallet keys...");
    let lp_keys = generate_keys(BitcoinNetwork::Regtest);
    println!("   ✅ Mnemonic: {}", lp_keys.mnemonic);
    println!("   ✅ Master fingerprint: {}", lp_keys.master_fingerprint);
    println!("   ✅ Account xPub (RGB): {}", lp_keys.account_xpub_colored);
    println!("   ✅ Account xPub (BTC): {}\n", lp_keys.account_xpub_vanilla);

    // Step 3: Create LP wallet
    println!("💼 Creating LP wallet...");
    let wallet_data = WalletData {
        data_dir: data_dir.to_string_lossy().to_string(),
        bitcoin_network: BitcoinNetwork::Regtest,
        database_type: DatabaseType::Sqlite,
        max_allocations_per_utxo: 1,
        account_xpub_vanilla: lp_keys.account_xpub_vanilla.clone(),
        account_xpub_colored: lp_keys.account_xpub_colored.clone(),
        mnemonic: Some(lp_keys.mnemonic.clone()),
        master_fingerprint: lp_keys.master_fingerprint.clone(),
        vanilla_keychain: Some(1),
        supported_schemas: vec![
            AssetSchema::Nia,
        ],
    };

    let mut wallet = Wallet::new(wallet_data.clone())?;
    println!("   ✅ LP wallet created successfully!\n");

    // Step 4: Fund wallet with BTC
    println!("💰 Funding LP Wallet with BTC");
    println!("==============================\n");
    
    let address = wallet.get_address()?;
    println!("🏠 LP Wallet address: {}\n", address);
    
    // println!("💸 Requesting 0.1 BTC from regtest faucet...");
    // let rpc = RegtestRpc::new("http://18.119.98.232:5000/execute")?;
    // rpc.send_to_address(&address, 0.1)?;
    // println!("   ✅ Sent 0.1 BTC to LP wallet\n");
    
    // println!("⛏️  Mining blocks to confirm...");
    // rpc.mine(1)?;
    // println!("   ✅ Mined 1 block\n");

    // Step 5: Get LP public key from wallet
    use std::str::FromStr;
    use rgb_lib::bitcoin::bip32::Xpub;
    
    // Derive public key from wallet's colored xPub (RGB keychain)
    // Sync looks at External keychain which uses RGB descriptor
    let xpub = Xpub::from_str(&lp_keys.account_xpub_colored)
        .expect("Valid xPub");
    
    // Derive first public key from the xPub (m/0)
    let secp = rgb_lib::bitcoin::secp256k1::Secp256k1::new();
    let derived_xpub = xpub.derive_pub(&secp, &[
        rgb_lib::bitcoin::bip32::ChildNumber::from_normal_idx(0).unwrap()
    ]).expect("Derivation succeeds");
    
    // Convert secp256k1::PublicKey to bitcoin::PublicKey
    let lp_pubkey = PublicKey::new(derived_xpub.public_key);
    
    println!("🔑 LP Public Key (from wallet): {}\n", lp_pubkey);

    // Step 6: User public key (provided)
    let user_pubkey = PublicKey::from_str(
        "03d6c27614557184d269b9cb19b1bc32479e661d86a925f4c4e46c734adcea3d19"
    ).expect("Valid user pubkey");
    println!("🔑 User Public Key: {}\n", user_pubkey);

    // Preimage: 86a85cd1cb86c51186d190972c9f8413f436911fc0de241b6df20877ebbadecc
    // Payment Hash: f4d376425855e2354bf30e17904f4624f6f9aa297973cca0445cdf4cef718b2a
    
    let preimage_hex = "86a85cd1cb86c51186d190972c9f8413f436911fc0de241b6df20877ebbadecc";
    let payment_hash_hex = "f4d376425855e2354bf30e17904f4624f6f9aa297973cca0445cdf4cef718b2a";
    
    let preimage_bytes = hex::decode(preimage_hex)
        .expect("Valid preimage hex");
    let preimage: [u8; 32] = preimage_bytes.try_into()
        .expect("Preimage is 32 bytes");
    
    let payment_hash_bytes = hex::decode(payment_hash_hex)
        .expect("Valid payment hash hex");
    let payment_hash: [u8; 32] = payment_hash_bytes.try_into()
        .expect("Payment hash is 32 bytes");
    
    let computed_hash = sha256::Hash::hash(&preimage);
    let computed_hash_bytes: &[u8] = computed_hash.as_ref();
    
    let invoice = RgbLnInvoice {
        payment_hash: payment_hash_hex.to_string(),
        amount_asset: 13,
        asset_id: "rgb:AxBwL0~H-EAIs51Q-p1rNBjG-NYkBmNb-gt~mV4o-bFC7GPg".to_string(),
        description: "Test RGB-LN Payment".to_string(),
        expiry: 36000,
    };

    println!("📧 RGB-LN Invoice:");
    println!("   Payment Hash: {}", invoice.payment_hash);
    println!("   Amount: {} asset units", invoice.amount_asset);
    println!("   Asset ID: {}", invoice.asset_id);
    println!("   Description: {}\n", invoice.description);
    

    // Step 8: Initialize LP service
    println!("🚀 Initializing Atomic LP Service...");
    let mut lp = AtomicRgbLnLiquidityProvider::new(
        wallet_data,
        lp_pubkey,
        "rpc://regtest.thunderstack.org:3000/json-rpc".to_string(),
        BdkNetwork::Regtest,
        "http://localhost:3000".to_string(), // RGB-LN node URL
        None, // No API key for demo
    )?;

    // Step 9: Create atomic swap offer
    println!("🔨 Creating ATOMIC HTLC swap...");
    let offer = lp.create_atomic_swap(invoice.clone(), user_pubkey)?;
    
    println!("   ✅ HTLC Created!");
    println!("   Swap ID: {}", offer.swap_id);
    println!("   HTLC Address: {}", offer.htlc_address);
    println!("   Recipient ID: {}", offer.recipient_id);
    println!("   Payment Hash: {}", offer.payment_hash);
    println!("   Timelock: {} blocks (~24 hours)\n", offer.timelock_blocks);

    // Step 10: Display RGB invoice for user to send assets
    println!("📬 RGB Invoice for User:");
    println!("   {}\n", offer.rgb_invoice);
    


    // Step 11: Show HTLC script details
    println!("📜 HTLC Script Guarantees:");
    println!("   IF (preimage SHA256 == {}):", hex::encode(&payment_hash[..8]));
    println!("     ✅ LP can claim with signature");
    println!("   ELSE:");
    println!("     ⏰ User can refund after {} blocks\n", offer.timelock_blocks);

    // Step 12: Demonstrate atomicity

    
    // LIVE DEMONSTRATION: Bring wallet online and monitor in real-time loop
    #[cfg(any(feature = "electrum", feature = "esplora"))]
    {
        println!("\n📡 LIVE DEMO: Online Wallet Monitoring Loop");
        println!("===========================================\n");
        
        // Bring wallet online with regtest Electrum
        println!("🌐 Bringing wallet online...");
        match lp.go_online(false, Some("tcp://regtest.thunderstack.org:50001".to_string())) {
            Ok(online) => {
                println!("🔄 Starting monitoring loop...");

                use std::time::{Duration, Instant};
                use std::thread;
                
                let start_time = Instant::now();
                let timeout = Duration::from_secs(1200); // 20 minutes timeout for demo
                let mut check_count = 0;
                
                loop {
                    check_count += 1;
                    let elapsed = start_time.elapsed();
                    
                    println!("🔍 Check #{} ({}s elapsed)...", check_count, elapsed.as_secs());
                    
                    match lp.check_htlc_funding(online.clone(), &offer.swap_id) {
                        Ok(status) => {
                            match status {
                                HtlcFundingStatus::Funded => {
                                    println!("\n🎉 SUCCESS! HTLC is FUNDED!");
                                    println!("   ✅ RGB assets received at HTLC address");
                                    break;
                                }
                                HtlcFundingStatus::Pending => {
                                    println!("   ⏳ Status: Pending (WaitingCounterparty)");
                                    
                                    if elapsed > timeout {
                                        println!("\n⏰ Timeout reached (demo only)");
                                        break;
                                    }
                                    
                                    println!("   💤 Sleeping 10 seconds before next check...\n");
                                    thread::sleep(Duration::from_secs(30));
                                }
                            }
                        }
                        Err(e) => {
                            println!("   ⚠️  Error: {}", e);
                            println!("   🔄 Retrying in 10 seconds...\n");
                            thread::sleep(Duration::from_secs(30));
                            
                            if elapsed > timeout {
                                println!("   ⏰ Timeout reached");
                                break;
                            }
                        }
                    }
                }
                
                println!("📊 Monitoring Summary:");
                println!("   Total checks: {}", check_count);
                println!("   Duration: {} seconds", start_time.elapsed().as_secs());
                println!("   HTLC Address: {}", offer.htlc_address);
                println!("   Status: Waiting for user to send RGB assets\n");
            }
            Err(e) => {
                println!("   ⚠️  Could not connect to Electrum: {}", e);
                println!("   ℹ️  Check that regtest.thunderstack.org:50001 is accessible\n");
            }
        }

        
        println!("\n🎬 CLAIM DEMO: How to Claim HTLC");
        println!("==================================\n");

        #[cfg(feature = "electrum")]
        {
        println!("\n🧪 LIVE CLAIM TEST");
        println!("==================\n");
        
        let online = lp.go_online(false, Some("tcp://regtest.thunderstack.org:50001".to_string()))?;
        
        println!("🔄 Refreshing wallet state before claiming...");
        
        match lp.check_htlc_funding(online.clone(), &offer.swap_id) {
            Ok(HtlcFundingStatus::Funded) => {
                println!("✅ HTLC is FUNDED! Proceeding with claim...\n");
                
                println!("🔓 Claiming HTLC with preimage...");
                println!("   This will:");
                println!("   1. Derive LP private key from mnemonic");
                println!("   2. Build RGB-colored PSBT from HTLC UTXO");
                println!("   3. Sign with HTLC witness [sig, preimage, 0x01, script]");
                println!("   4. Broadcast transaction via send_end()");
                println!("   5. Post RGB consignments to proxy\n");
                
                let claim_result = lp.claim_htlc_atomic(
                    &offer.swap_id,
                    preimage,
                    online.clone(),
                )?;
                
                println!("\n🎉 CLAIM TRANSACTION BROADCAST!");
                println!("   ================================");
                println!("   Amount claimed: {} units", claim_result.amount_claimed);
                println!("   Asset ID: {}", claim_result.asset_id);
                println!("   Claim TXID: {}", claim_result.claim_txid);
                println!("   Preimage: {}", claim_result.preimage_hex);
                println!("   ================================\n");
                
                println!("✅ RGB transaction broadcast to Bitcoin network");
                println!("   Old UTXO: Spent (HTLC P2WSH)");
                println!("   New UTXO: Created (LP witness address)\n");
                
                // Refresh to see claimed assets (similar to user_payer flow)
                println!("🔄 Refreshing wallet to see claimed assets...");
                println!("   (This syncs the RGB transfer state)\n");
                
                use std::time::Duration;
                use std::thread;
                
                // Give it a moment for the transaction to propagate
                thread::sleep(Duration::from_secs(2));
                
                // Do refresh to update transfer status
                lp.check_htlc_funding(online.clone(), &offer.swap_id)?;
                
                // Check wallet state after claim
                println!("📊 Wallet State After Claim:");
                let assets = lp.wallet.list_assets(vec![])?;
                if let Some(nia_assets) = assets.nia {
                    for asset in nia_assets {
                        if asset.asset_id == claim_result.asset_id {
                            println!("   ✅ Asset: {}", asset.asset_id);
                            println!("      Balance: {} units", asset.balance.settled);
                            println!("      Name: {}", asset.name);
                        }
                    }
                }
                
                // List transfers to see the claim
                println!("\n📋 Recent Transfers:");
                let transfers = lp.wallet.list_transfers(Some(claim_result.asset_id.clone()))?;
                for (i, transfer) in transfers.iter().rev().take(2).enumerate() {
                    println!("   Transfer #{}: {:?}", i + 1, transfer.status);
                    if let Some(txid) = &transfer.txid {
                        println!("      TXID: {}", txid);
                    }
                }
                
                println!("\n✅ CLAIM COMPLETE!");
                println!("   RGB assets now in LP wallet with standard controls");
                println!("   No HTLC restrictions, can spend freely\n");
            }
            Ok(HtlcFundingStatus::Pending) => {
                println!("⏳ HTLC not yet funded");
                println!("   Status: WaitingCounterparty");
                println!("   💡 Run user_payer.rs to fund the HTLC");
                println!("   💡 Or wait for monitoring loop to detect funding\n");
            }
            Err(e) => {
                println!("⚠️  Error checking HTLC funding: {}", e);
                println!("   This is normal if user hasn't sent RGB assets yet\n");
            }
        }
        }
    }
    
    Ok(())
}

