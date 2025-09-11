use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceChange {
    /// Amount change (negative = outgoing, positive = incoming)
    pub amount: i128,
    
    /// Token symbol
    pub symbol: String,
    
    /// Token name
    pub name: String,
    
    /// Token decimals
    pub decimals: u32,
    
    /// Token mint address
    pub address: String,
    
    /// Token logo URI
    #[serde(rename = "logoURI")]
    pub logo_uri: Option<String>,
    
    /// Whether token uses scaled UI amounts
    #[serde(rename = "isScaledUiToken")]
    pub is_scaled_ui_token: bool,
    
    /// Multiplier for scaled tokens
    pub multiplier: Option<f64>,
    
    /// Token account address (for createAssociatedAccount transactions)
    #[serde(rename = "tokenAccount")]
    pub token_account: Option<String>,
    
    /// Owner wallet address (for createAssociatedAccount transactions)
    pub owner: Option<String>,
    
    /// Program ID (for createAssociatedAccount transactions) 
    #[serde(rename = "programId")]
    pub program_id: Option<String>,
}

fn main() {
    // Test SOL balance change (no tokenAccount/owner/programId)
    let sol_json = r#"{
        "amount": -6400006425,
        "symbol": "SOL",
        "name": "Wrapped SOL",
        "decimals": 9,
        "address": "So11111111111111111111111111111111111111112",
        "logoURI": "https://raw.githubusercontent.com/solana-labs/token-list/main/assets/mainnet/So11111111111111111111111111111111111111112/logo.png",
        "isScaledUiToken": false,
        "multiplier": null
    }"#;
    
    // Test MASHA balance change (has both sets of fields)
    let masha_json = r#"{
        "tokenAccount": "8EaionCKdoH9nrvVGYkEr26Mz8Q2YR8WTZeNSPF75gtz",
        "owner": "5ngDQ3vMT7GHeFvXP5GLX79t6fqG2t15kyQTtst9zFdw",
        "decimals": 6,
        "programId": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
        "amount": 50510989591,
        "address": "mae8vJGf8Wju8Ron1oDTQVaTGGBpcpWDwoRQJALMMf2",
        "name": "Masha",
        "symbol": "MASHA",
        "logoURI": "https://general-inventory.coin98.tech/file/ins/masha.png",
        "isScaledUiToken": false,
        "multiplier": null
    }"#;
    
    // Test both balance changes
    println!("Testing SOL balance change deserialization...");
    match serde_json::from_str::<BalanceChange>(sol_json) {
        Ok(sol_balance) => {
            println!("✅ SOL balance change parsed successfully: {:?}", sol_balance.symbol);
            println!("   Token account: {:?}", sol_balance.token_account);
            println!("   Owner: {:?}", sol_balance.owner);
            println!("   Program ID: {:?}", sol_balance.program_id);
        }
        Err(e) => {
            println!("❌ SOL balance change failed to parse: {}", e);
        }
    }
    
    println!("\nTesting MASHA balance change deserialization...");
    match serde_json::from_str::<BalanceChange>(masha_json) {
        Ok(masha_balance) => {
            println!("✅ MASHA balance change parsed successfully: {:?}", masha_balance.symbol);
            println!("   Token account: {:?}", masha_balance.token_account);
            println!("   Owner: {:?}", masha_balance.owner);
            println!("   Program ID: {:?}", masha_balance.program_id);
        }
        Err(e) => {
            println!("❌ MASHA balance change failed to parse: {}", e);
        }
    }
    
    // Test mixed array
    let mixed_array_json = format!("[{}, {}]", sol_json, masha_json);
    println!("\nTesting mixed array deserialization...");
    match serde_json::from_str::<Vec<BalanceChange>>(&mixed_array_json) {
        Ok(balance_changes) => {
            println!("✅ Mixed array parsed successfully: {} items", balance_changes.len());
            for (i, change) in balance_changes.iter().enumerate() {
                println!("  Item {}: {} - {} (amount: {}, token_account: {:?})", 
                    i, change.symbol, change.name, change.amount, change.token_account);
            }
        }
        Err(e) => {
            println!("❌ Mixed array failed to parse: {}", e);
        }
    }
}