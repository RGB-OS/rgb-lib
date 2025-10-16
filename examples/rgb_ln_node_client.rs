// Example: RGB-LN Node Client Interface
//
// This shows what your RGB-LN node needs to expose to the LP service.
// The LP service calls your RGB-LN node, which handles all Lightning complexity.

use serde::{Deserialize, Serialize};

/// RGB Lightning Network node client trait
/// 
/// Your RGB-LN node just needs to implement this simple interface.
/// The LP service will call it to pay invoices and get preimages back.
pub trait RgbLnNode: Send + Sync {
    /// Pay an RGB-LN invoice
    /// 
    /// # Arguments
    /// * `invoice` - The RGB-LN invoice (payment hash)
    /// * `asset_id` - RGB asset contract ID
    /// * `amount` - Amount in asset units
    /// * `timeout_seconds` - How long to wait for payment
    /// 
    /// # Returns
    /// * `Ok(RgbLnPaymentResult)` - Payment succeeded or failed with details
    /// * `Err(RgbLnError)` - Error communicating with node
    fn pay_invoice(
        &self,
        invoice: &str,
        asset_id: &str,
        amount: u64,
        timeout_seconds: u64,
    ) -> Result<RgbLnPaymentResult, RgbLnError>;
    
    /// Check if the node is ready to make payments
    fn is_ready(&self) -> bool;
    
    /// Get node info (optional, for monitoring)
    fn get_info(&self) -> Result<RgbLnNodeInfo, RgbLnError>;
}

/// Result of paying an RGB-LN invoice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RgbLnPaymentResult {
    /// Whether payment succeeded
    pub success: bool,
    
    /// Preimage (32 bytes) - ONLY present if success = true
    /// This is what you use to claim the on-chain RGB assets
    pub preimage: Option<[u8; 32]>,
    
    /// Payment hash (32 bytes)
    pub payment_hash: [u8; 32],
    
    /// Amount actually paid (may differ from requested due to routing)
    pub paid_amount: Option<u64>,
    
    /// Fees paid in millisatoshis
    pub fees_paid_msat: Option<u64>,
    
    /// Error message if failed
    pub error: Option<String>,
}

/// Errors from RGB-LN node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RgbLnError {
    /// Could not find a route to destination
    RouteNotFound,
    
    /// Not enough liquidity in channels
    InsufficientLiquidity,
    
    /// Payment timed out
    PaymentTimeout,
    
    /// Invoice is invalid or malformed
    InvalidInvoice,
    
    /// Network communication error
    NetworkError(String),
    
    /// Node is not ready (syncing, offline, etc)
    NodeNotReady,
    
    /// Other error
    Other(String),
}

/// Information about the RGB-LN node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RgbLnNodeInfo {
    /// Node public key
    pub node_pubkey: String,
    
    /// Number of active channels
    pub num_channels: u32,
    
    /// Total local balance across all channels
    pub total_local_balance: u64,
    
    /// Whether node is synced with network
    pub synced: bool,
    
    /// Supported RGB assets
    pub supported_assets: Vec<String>,
}

// ============================================================================
// HTTP Client Implementation Example
// ============================================================================

use reqwest::blocking::Client;

/// HTTP client for RGB-LN node
pub struct RgbLnHttpClient {
    base_url: String,
    api_key: String,
    client: Client,
}

impl RgbLnHttpClient {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            base_url,
            api_key,
            client: Client::new(),
        }
    }
}

impl RgbLnNode for RgbLnHttpClient {
    fn pay_invoice(
        &self,
        invoice: &str,
        asset_id: &str,
        amount: u64,
        timeout_seconds: u64,
    ) -> Result<RgbLnPaymentResult, RgbLnError> {
        #[derive(Serialize)]
        struct PayRequest {
            invoice: String,
            asset_id: String,
            amount: u64,
            timeout_seconds: u64,
        }
        
        let request = PayRequest {
            invoice: invoice.to_string(),
            asset_id: asset_id.to_string(),
            amount,
            timeout_seconds,
        };
        
        let response = self.client
            .post(&format!("{}/v1/pay", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .timeout(std::time::Duration::from_secs(timeout_seconds + 10))
            .send()
            .map_err(|e| RgbLnError::NetworkError(e.to_string()))?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(RgbLnError::NetworkError(
                format!("HTTP {}: {}", status, error_text)
            ));
        }
        
        let result: RgbLnPaymentResult = response.json()
            .map_err(|e| RgbLnError::NetworkError(
                format!("Failed to parse response: {}", e)
            ))?;
        
        Ok(result)
    }
    
    fn is_ready(&self) -> bool {
        self.client
            .get(&format!("{}/v1/health", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
    
    fn get_info(&self) -> Result<RgbLnNodeInfo, RgbLnError> {
        let response = self.client
            .get(&format!("{}/v1/info", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .map_err(|e| RgbLnError::NetworkError(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(RgbLnError::NetworkError(
                format!("HTTP {}", response.status())
            ));
        }
        
        let info: RgbLnNodeInfo = response.json()
            .map_err(|e| RgbLnError::NetworkError(e.to_string()))?;
        
        Ok(info)
    }
}

// ============================================================================
// Mock Implementation for Testing
// ============================================================================

/// Mock RGB-LN node for testing
pub struct MockRgbLnNode {
    /// Whether payments should succeed
    pub should_succeed: bool,
    /// Simulated delay in milliseconds
    pub delay_ms: u64,
}

impl MockRgbLnNode {
    pub fn new_success() -> Self {
        Self {
            should_succeed: true,
            delay_ms: 100,
        }
    }
    
    pub fn new_failure() -> Self {
        Self {
            should_succeed: false,
            delay_ms: 50,
        }
    }
}

impl RgbLnNode for MockRgbLnNode {
    fn pay_invoice(
        &self,
        invoice: &str,
        _asset_id: &str,
        amount: u64,
        _timeout_seconds: u64,
    ) -> Result<RgbLnPaymentResult, RgbLnError> {
        // Simulate processing delay
        std::thread::sleep(std::time::Duration::from_millis(self.delay_ms));
        
        // Parse payment hash from invoice
        let payment_hash = hex::decode(invoice)
            .map_err(|_| RgbLnError::InvalidInvoice)?;
        let payment_hash: [u8; 32] = payment_hash.try_into()
            .map_err(|_| RgbLnError::InvalidInvoice)?;
        
        if self.should_succeed {
            // Generate a deterministic "preimage" for testing
            // WARNING: This is NOT how real preimages work!
            // In production, preimage is revealed by the invoice recipient
            let mut preimage = payment_hash;
            preimage.reverse(); // Just make it different from hash
            
            Ok(RgbLnPaymentResult {
                success: true,
                preimage: Some(preimage),
                payment_hash,
                paid_amount: Some(amount),
                fees_paid_msat: Some(1000), // 1 sat
                error: None,
            })
        } else {
            Ok(RgbLnPaymentResult {
                success: false,
                preimage: None,
                payment_hash,
                paid_amount: None,
                fees_paid_msat: None,
                error: Some("Simulated payment failure".to_string()),
            })
        }
    }
    
    fn is_ready(&self) -> bool {
        true
    }
    
    fn get_info(&self) -> Result<RgbLnNodeInfo, RgbLnError> {
        Ok(RgbLnNodeInfo {
            node_pubkey: "mock_node_pubkey".to_string(),
            num_channels: 5,
            total_local_balance: 100_000,
            synced: true,
            supported_assets: vec!["rgb:test:asset".to_string()],
        })
    }
}

// ============================================================================
// Usage Example
// ============================================================================

fn main() {
    println!("RGB-LN Node Client Interface Example\n");
    
    // Example 1: Using HTTP client
    println!("=== HTTP Client Example ===");
    let http_client = RgbLnHttpClient::new(
        "http://localhost:8080".to_string(),
        "your-api-key".to_string(),
    );
    
    println!("Client created. Base URL: http://localhost:8080");
    println!("Check if ready: {}", http_client.is_ready());
    println!();
    
    // Example 2: Using mock for testing
    println!("=== Mock Client Example (Success) ===");
    let mock_success = MockRgbLnNode::new_success();
    
    let test_invoice = hex::encode([1u8; 32]); // Fake payment hash
    match mock_success.pay_invoice(
        &test_invoice,
        "rgb:test:asset",
        1000,
        300,
    ) {
        Ok(result) => {
            println!("✅ Payment result:");
            println!("   Success: {}", result.success);
            println!("   Preimage: {:?}", result.preimage.map(hex::encode));
            println!("   Paid amount: {:?}", result.paid_amount);
            println!("   Fees: {:?} msat", result.fees_paid_msat);
        }
        Err(e) => {
            println!("❌ Error: {:?}", e);
        }
    }
    println!();
    
    println!("=== Mock Client Example (Failure) ===");
    let mock_failure = MockRgbLnNode::new_failure();
    
    match mock_failure.pay_invoice(
        &test_invoice,
        "rgb:test:asset",
        1000,
        300,
    ) {
        Ok(result) => {
            println!("Payment result:");
            println!("   Success: {}", result.success);
            println!("   Error: {:?}", result.error);
        }
        Err(e) => {
            println!("❌ Error: {:?}", e);
        }
    }
    println!();
    
    // Example 3: Node info
    println!("=== Node Info ===");
    if let Ok(info) = mock_success.get_info() {
        println!("Node pubkey: {}", info.node_pubkey);
        println!("Channels: {}", info.num_channels);
        println!("Local balance: {}", info.total_local_balance);
        println!("Synced: {}", info.synced);
        println!("Supported assets: {:?}", info.supported_assets);
    }
    println!();
    
    println!("✅ This interface is all you need to implement!");
    println!("   Your RGB-LN node exposes this API,");
    println!("   Your LP service calls it to pay invoices.");
}

