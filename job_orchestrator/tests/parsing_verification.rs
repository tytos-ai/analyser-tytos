//! Critical test to verify our parsing logic against actual BirdEye data

use dex_client::{GeneralTraderTransaction, GeneralTraderTransactionsResponse};
use job_orchestrator::ProcessedSwap;
use serde_json;
use std::collections::HashMap;

#[tokio::test]
async fn test_birdeye_parsing_accuracy() {
    // Load the actual transaction data
    let json_data = r#"
    {
      "success": true,
      "data": {
        "items": [
          {
            "quote": {
              "symbol": "SOL",
              "decimals": 9,
              "address": "So11111111111111111111111111111111111111112",
              "amount": 8879025300500,
              "type": "transfer",
              "type_swap": "from",
              "ui_amount": 8879.0253005,
              "price": 146.96600120510325,
              "nearest_price": 146.96600120510325,
              "change_amount": -8879025300500,
              "ui_change_amount": -8879.0253005,
              "fee_info": null
            },
            "base": {
              "symbol": "BNSOL",
              "decimals": 9,
              "address": "BNso1VUJnh4zcfpZa6986Ea66P6TCp59hvtNJ8b1X85",
              "amount": 8369439226307,
              "type": "mintTo",
              "type_swap": "to",
              "ui_amount": 8369.439226307,
              "price": 155.5613863476638,
              "nearest_price": 155.5613863476638,
              "change_amount": 8369439226307,
              "ui_change_amount": 8369.439226307,
              "fee_info": null
            },
            "base_price": 155.5613863476638,
            "quote_price": 146.96600120510325,
            "tx_hash": "58Y6ScVvkFutzKp57dX5xfLfxvw6e9pMYeK5vBbAb3fWKLBTpKvSxJZYQLGQDSkA1w3J8hF2gcPKgfr8sjpCBk5U",
            "source": "pumpfun",
            "block_unix_time": 1751414738,
            "tx_type": "swap",
            "address": "",
            "owner": "GBJ4MZe8fqpA6UVgjh19BwJPMb79KDfMv78XnFVxgH2Q"
          }
        ]
      }
    }"#;

    println!("🚨 CRITICAL RUST PARSING VERIFICATION");
    println!("=====================================");

    // Parse using our actual Rust structs
    let response: GeneralTraderTransactionsResponse =
        serde_json::from_str(json_data).expect("Failed to parse transaction data");

    assert!(response.success, "Response should be successful");
    assert_eq!(
        response.data.items.len(),
        1,
        "Should have exactly 1 transaction"
    );

    let tx = &response.data.items[0];

    println!("\n📊 TESTING TRANSACTION PARSING:");
    println!("TX Hash: {}...", &tx.tx_hash[..12]);

    // Test our aggregation logic using ProcessedSwap
    let processed_swaps = ProcessedSwap::from_birdeye_transactions(&response.data.items)
        .expect("Failed to process swaps");

    assert_eq!(
        processed_swaps.len(),
        1,
        "Should produce exactly 1 processed swap"
    );

    let swap = &processed_swaps[0];

    println!("\n🔍 PROCESSED SWAP RESULT:");
    println!(
        "  token_in: {}... ({})",
        &swap.token_in[..8],
        swap.amount_in
    );
    println!(
        "  token_out: {}... ({})",
        &swap.token_out[..8],
        swap.amount_out
    );
    println!("  sol_equivalent: {}", swap.sol_equivalent);
    println!("  price_per_token: ${:.6}", swap.price_per_token);

    // Verify the aggregation logic
    let sol_mint = "So11111111111111111111111111111111111111112";
    let bnsol_mint = "BNso1VUJnh4zcfpZa6986Ea66P6TCp59hvtNJ8b1X85";

    // This should be a SOL → BNSOL swap (BUY BNSOL)
    assert_eq!(swap.token_in, sol_mint, "token_in should be SOL");
    assert_eq!(swap.token_out, bnsol_mint, "token_out should be BNSOL");

    // Verify amounts match exactly
    let expected_sol_spent = 8879.0253005;
    let expected_bnsol_received = 8369.439226307;
    let expected_price = 155.5613863476638;

    assert!(
        (swap.amount_in.to_string().parse::<f64>().unwrap() - expected_sol_spent).abs() < 0.001,
        "SOL amount should match exactly"
    );
    assert!(
        (swap.amount_out.to_string().parse::<f64>().unwrap() - expected_bnsol_received).abs()
            < 0.001,
        "BNSOL amount should match exactly"
    );
    assert!(
        (swap.price_per_token.to_string().parse::<f64>().unwrap() - expected_price).abs() < 0.001,
        "Price should match exactly"
    );

    // Test FinancialEvent creation
    let financial_event = swap.to_financial_event("GBJ4MZe8fqpA6UVgjh19BwJPMb79KDfMv78XnFVxgH2Q");

    println!("\n🎯 FINANCIAL EVENT:");
    println!("  event_type: {:?}", financial_event.event_type);
    println!("  token_mint: {}...", &financial_event.token_mint[..8]);
    println!("  token_amount: {}", financial_event.token_amount);
    println!("  sol_amount: {}", financial_event.sol_amount);
    if let Some(price) = &financial_event.metadata.price_per_token {
        println!("  price_per_token: ${:.6}", price);
    }

    // Verify FinancialEvent is correct
    use pnl_core::EventType;
    assert_eq!(
        financial_event.event_type,
        EventType::Buy,
        "Should be a Buy event"
    );
    assert_eq!(
        financial_event.token_mint, bnsol_mint,
        "Should be buying BNSOL"
    );

    // Amounts should match our expectations
    assert!(
        (financial_event
            .token_amount
            .to_string()
            .parse::<f64>()
            .unwrap()
            - expected_bnsol_received)
            .abs()
            < 0.001,
        "FinancialEvent token_amount should match"
    );
    assert!(
        (financial_event
            .sol_amount
            .to_string()
            .parse::<f64>()
            .unwrap()
            - expected_sol_spent)
            .abs()
            < 0.001,
        "FinancialEvent sol_amount should match"
    );

    if let Some(price) = &financial_event.metadata.price_per_token {
        assert!(
            (price.to_string().parse::<f64>().unwrap() - expected_price).abs() < 0.001,
            "FinancialEvent price should match"
        );
    }

    println!("\n✅ VERIFICATION COMPLETE:");
    println!("✅ Our Rust parsing logic produces correct results!");
    println!("✅ Transaction aggregation works correctly");
    println!("✅ FinancialEvent creation is accurate");
    println!("✅ All amounts and prices match expected values");
}

#[tokio::test]
async fn test_sell_transaction_parsing() {
    // Test a SELL transaction (BNSOL → SOL)
    let json_data = r#"
    {
      "success": true,
      "data": {
        "items": [
          {
            "quote": {
              "symbol": "SOL",
              "decimals": 9,
              "address": "So11111111111111111111111111111111111111112",
              "amount": 1201603133996,
              "type": "transfer",
              "type_swap": "to",
              "ui_amount": 1201.603133996,
              "price": 141.21868624749953,
              "nearest_price": 141.21868624749953,
              "change_amount": 1201603133996,
              "ui_change_amount": 1201.603133996,
              "fee_info": null
            },
            "base": {
              "symbol": "BNSOL",
              "decimals": 9,
              "address": "BNso1VUJnh4zcfpZa6986Ea66P6TCp59hvtNJ8b1X85",
              "amount": 1134869983080,
              "type": "burn",
              "type_swap": "from",
              "ui_amount": 1134.86998308,
              "price": 149.6815837399493,
              "nearest_price": 149.6815837399493,
              "change_amount": -1134869983080,
              "ui_change_amount": -1134.86998308,
              "fee_info": null
            },
            "base_price": 149.6815837399493,
            "quote_price": 141.21868624749953,
            "tx_hash": "BKZwsYfg8R6TtestSELLtransactionHash",
            "source": "pumpfun",
            "block_unix_time": 1751414800,
            "tx_type": "swap",
            "address": "",
            "owner": "GBJ4MZe8fqpA6UVgjh19BwJPMb79KDfMv78XnFVxgH2Q"
          }
        ]
      }
    }"#;

    println!("\n🚨 TESTING SELL TRANSACTION");
    println!("===========================");

    let response: GeneralTraderTransactionsResponse =
        serde_json::from_str(json_data).expect("Failed to parse sell transaction");

    let processed_swaps = ProcessedSwap::from_birdeye_transactions(&response.data.items)
        .expect("Failed to process sell swap");

    assert_eq!(processed_swaps.len(), 1);
    let swap = &processed_swaps[0];

    let sol_mint = "So11111111111111111111111111111111111111112";
    let bnsol_mint = "BNso1VUJnh4zcfpZa6986Ea66P6TCp59hvtNJ8b1X85";

    // This should be a BNSOL → SOL swap (SELL BNSOL)
    assert_eq!(swap.token_in, bnsol_mint, "token_in should be BNSOL");
    assert_eq!(swap.token_out, sol_mint, "token_out should be SOL");

    let financial_event = swap.to_financial_event("GBJ4MZe8fqpA6UVgjh19BwJPMb79KDfMv78XnFVxgH2Q");

    use pnl_core::EventType;
    assert_eq!(
        financial_event.event_type,
        EventType::Sell,
        "Should be a Sell event"
    );
    assert_eq!(
        financial_event.token_mint, bnsol_mint,
        "Should be selling BNSOL"
    );

    println!("✅ SELL transaction parsing works correctly!");
}
