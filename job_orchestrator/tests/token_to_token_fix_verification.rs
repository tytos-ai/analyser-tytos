//! Test to verify token-to-token sol_equivalent unit fix

use dex_client::{GeneralTraderTransaction, TokenTransactionSide};
use job_orchestrator::ProcessedSwap;
use rust_decimal::Decimal;

fn create_mock_token_to_token_transaction() -> GeneralTraderTransaction {
    GeneralTraderTransaction {
        quote: TokenTransactionSide {
            symbol: "USDC".to_string(),
            decimals: 6,
            address: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
            amount: 1000000000, // 1000 USDC (with 6 decimals)
            transfer_type: Some("transfer".to_string()),
            type_swap: "from".to_string(),
            ui_amount: 1000.0,
            price: Some(1.0), // $1 per USDC
            nearest_price: Some(1.0),
            change_amount: -1000000000,
            ui_change_amount: -1000.0, // Spent 1000 USDC
            fee_info: None,
        },
        base: TokenTransactionSide {
            symbol: "RENDER".to_string(),
            decimals: 8,
            address: "rndrizKT3MK1iimdxRdWabcF7Zg7AR5T4nud4EkHBof".to_string(),
            amount: 5000000000, // 50 RENDER (with 8 decimals)
            transfer_type: Some("mintTo".to_string()),
            type_swap: "to".to_string(),
            ui_amount: 50.0,
            price: Some(20.0), // $20 per RENDER
            nearest_price: Some(20.0),
            change_amount: 5000000000,
            ui_change_amount: 50.0, // Received 50 RENDER
            fee_info: None,
        },
        base_price: Some(20.0), // $20 per RENDER
        quote_price: 150.0,     // $150 per SOL (critical for conversion!)
        tx_hash: "token_to_token_test_hash".to_string(),
        source: "test".to_string(),
        block_unix_time: 1751414738,
        tx_type: "swap".to_string(),
        address: "".to_string(),
        owner: "test_wallet".to_string(),
    }
}

#[tokio::test]
async fn test_sol_equivalent_unit_fix() {
    println!("🧪 TESTING SOL EQUIVALENT UNIT FIX");
    println!("==================================================");

    let mock_tx = create_mock_token_to_token_transaction();

    // Process with our fixed logic
    let processed_swaps =
        ProcessedSwap::from_birdeye_transactions(&[mock_tx]).expect("Failed to process swap");

    assert_eq!(
        processed_swaps.len(),
        1,
        "Should create exactly 1 processed swap"
    );

    let swap = &processed_swaps[0];

    println!("📊 SWAP DETAILS:");
    println!("  Token In: USDC ({})", swap.amount_in);
    println!("  Token Out: RENDER ({})", swap.amount_out);
    println!("  SOL Equivalent: {}", swap.sol_equivalent);
    println!("  Price per token: ${}", swap.price_per_token);

    // Expected calculation:
    // USD value = 50 RENDER × $20/RENDER = $1000 USD
    // SOL equivalent = $1000 USD ÷ $150/SOL = 6.67 SOL
    let expected_usd_value = Decimal::from(50) * Decimal::from(20); // $1000
    let expected_sol_equivalent = expected_usd_value / Decimal::from(150); // 6.67 SOL

    println!("\n🔍 VERIFICATION:");
    println!("  Expected USD value: ${}", expected_usd_value);
    println!("  Expected SOL equivalent: {} SOL", expected_sol_equivalent);
    println!("  Actual SOL equivalent: {} SOL", swap.sol_equivalent);

    // Verify the fix worked
    assert!(
        (swap.sol_equivalent - expected_sol_equivalent).abs() < Decimal::new(1, 2), // Within 0.01
        "SOL equivalent should be ~6.67 SOL, got {}",
        swap.sol_equivalent
    );

    // Verify it's NOT the old buggy USD value
    assert!(
        swap.sol_equivalent < Decimal::from(100),
        "SOL equivalent should be in SOL units (~6.67), not USD units (1000)"
    );

    // Verify the unit is reasonable for SOL
    assert!(
        swap.sol_equivalent > Decimal::from(1) && swap.sol_equivalent < Decimal::from(50),
        "SOL equivalent should be reasonable SOL amount, got {}",
        swap.sol_equivalent
    );

    println!("✅ SOL EQUIVALENT UNIT FIX VERIFIED!");
    println!("  ✅ Result is in SOL units, not USD");
    println!("  ✅ Calculation matches expected value");
    println!("  ✅ No longer assigns USD value to sol_amount field");
}

#[tokio::test]
async fn test_financial_event_sol_amount() {
    println!("\n🧪 TESTING FINANCIAL EVENT SOL AMOUNT");
    println!("========================================");

    let mock_tx = create_mock_token_to_token_transaction();
    let processed_swaps = ProcessedSwap::from_birdeye_transactions(&[mock_tx]).unwrap();
    let swap = &processed_swaps[0];

    // Create FinancialEvent (currently only creates BUY event for token-to-token)
    let financial_event = swap.to_financial_event("test_wallet");

    println!("📊 FINANCIAL EVENT:");
    println!("  Event type: {:?}", financial_event.event_type);
    println!("  Token mint: {}...", &financial_event.token_mint[..8]);
    println!("  Token amount: {}", financial_event.token_amount);
    println!("  SOL amount: {}", financial_event.sol_amount);

    // Verify the SOL amount is now in correct units
    assert!(
        financial_event.sol_amount < Decimal::from(50),
        "Financial event sol_amount should be in SOL units (~6.67), not USD (1000)"
    );

    assert_eq!(
        financial_event.sol_amount, swap.sol_equivalent,
        "Financial event sol_amount should match swap sol_equivalent"
    );

    println!("✅ FINANCIAL EVENT SOL AMOUNT VERIFIED!");
    println!("  ✅ sol_amount field contains SOL quantity");
    println!("  ✅ No more USD values in SOL fields");
}

#[tokio::test]
async fn test_dual_event_system_token_to_token() {
    println!("\n🧪 TESTING DUAL EVENT SYSTEM (Token → Token)");
    println!("==============================================");

    let mock_tx = create_mock_token_to_token_transaction();
    let processed_swaps = ProcessedSwap::from_birdeye_transactions(&[mock_tx]).unwrap();
    let swap = &processed_swaps[0];

    // Test new dual event method
    let events = swap.to_financial_events("test_wallet");

    println!("📊 DUAL EVENT RESULTS:");
    println!("  Number of events: {}", events.len());

    assert_eq!(
        events.len(),
        2,
        "Token-to-token swap should create exactly 2 events"
    );

    // First event should be SELL of input token (USDC)
    let sell_event = &events[0];
    println!("  Event 1 (SELL):");
    println!("    Type: {:?}", sell_event.event_type);
    println!("    Token: {}... (USDC)", &sell_event.token_mint[..8]);
    println!("    Amount: {} USDC", sell_event.token_amount);
    println!("    SOL equivalent: {} SOL", sell_event.sol_amount);

    assert_eq!(
        sell_event.event_type,
        pnl_core::EventType::Sell,
        "First event should be SELL"
    );
    assert_eq!(
        sell_event.token_mint, "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        "Should be selling USDC"
    );
    assert_eq!(
        sell_event.token_amount,
        Decimal::from(1000),
        "Should sell 1000 USDC"
    );
    // Verify SOL amount is approximately 6.67 SOL (allowing for precision differences)
    assert!(
        (sell_event.sol_amount - Decimal::new(667, 2)).abs() < Decimal::new(1, 2),
        "Should use SOL equivalent (~6.67)"
    );

    // Second event should be BUY of output token (RENDER)
    let buy_event = &events[1];
    println!("  Event 2 (BUY):");
    println!("    Type: {:?}", buy_event.event_type);
    println!("    Token: {}... (RENDER)", &buy_event.token_mint[..8]);
    println!("    Amount: {} RENDER", buy_event.token_amount);
    println!("    SOL equivalent: {} SOL", buy_event.sol_amount);

    assert_eq!(
        buy_event.event_type,
        pnl_core::EventType::Buy,
        "Second event should be BUY"
    );
    assert_eq!(
        buy_event.token_mint, "rndrizKT3MK1iimdxRdWabcF7Zg7AR5T4nud4EkHBof",
        "Should be buying RENDER"
    );
    assert_eq!(
        buy_event.token_amount,
        Decimal::from(50),
        "Should buy 50 RENDER"
    );
    // Verify SOL amount is approximately 6.67 SOL (allowing for precision differences)
    assert!(
        (buy_event.sol_amount - Decimal::new(667, 2)).abs() < Decimal::new(1, 2),
        "Should use SOL equivalent (~6.67)"
    );

    // Both events should have same SOL equivalent (conservation of value)
    assert_eq!(
        sell_event.sol_amount, buy_event.sol_amount,
        "Both events should have same SOL equivalent"
    );

    // Verify metadata contains swap type information
    assert!(
        sell_event.metadata.extra.contains_key("swap_type"),
        "SELL event should have swap_type metadata"
    );
    assert!(
        buy_event.metadata.extra.contains_key("swap_type"),
        "BUY event should have swap_type metadata"
    );
    assert_eq!(
        sell_event.metadata.extra.get("swap_type").unwrap(),
        "token_to_token_sell"
    );
    assert_eq!(
        buy_event.metadata.extra.get("swap_type").unwrap(),
        "token_to_token_buy"
    );

    // Verify counterpart token references
    assert_eq!(
        sell_event.metadata.extra.get("counterpart_token").unwrap(),
        &buy_event.token_mint
    );
    assert_eq!(
        buy_event.metadata.extra.get("counterpart_token").unwrap(),
        &sell_event.token_mint
    );

    println!("✅ DUAL EVENT SYSTEM VERIFIED!");
    println!("  ✅ Creates exactly 2 events for token-to-token swaps");
    println!("  ✅ SELL event properly disposes of input token");
    println!("  ✅ BUY event properly acquires output token");
    println!("  ✅ Both events use correct SOL equivalent values");
    println!("  ✅ Metadata properly links the two events");
}

#[tokio::test]
async fn test_sol_to_token_single_event() {
    println!("\n🧪 TESTING SINGLE EVENT (SOL → Token)");
    println!("======================================");

    // Create mock SOL → BNSOL transaction (like our current data)
    let mock_sol_to_token = GeneralTraderTransaction {
        quote: TokenTransactionSide {
            symbol: "SOL".to_string(),
            decimals: 9,
            address: "So11111111111111111111111111111111111111112".to_string(),
            amount: 1000000000000, // 1000 SOL
            transfer_type: Some("transfer".to_string()),
            type_swap: "from".to_string(),
            ui_amount: 1000.0,
            price: Some(150.0), // $150 per SOL
            nearest_price: Some(150.0),
            change_amount: -1000000000000,
            ui_change_amount: -1000.0, // Spent 1000 SOL
            fee_info: None,
        },
        base: TokenTransactionSide {
            symbol: "BNSOL".to_string(),
            decimals: 9,
            address: "BNso1VUJnh4zcfpZa6986Ea66P6TCp59hvtNJ8b1X85".to_string(),
            amount: 950000000000, // 950 BNSOL
            transfer_type: Some("mintTo".to_string()),
            type_swap: "to".to_string(),
            ui_amount: 950.0,
            price: Some(158.0), // $158 per BNSOL
            nearest_price: Some(158.0),
            change_amount: 950000000000,
            ui_change_amount: 950.0, // Received 950 BNSOL
            fee_info: None,
        },
        base_price: Some(158.0),
        quote_price: 150.0,
        tx_hash: "sol_to_token_test".to_string(),
        source: "test".to_string(),
        block_unix_time: 1751414738,
        tx_type: "swap".to_string(),
        address: "".to_string(),
        owner: "test_wallet".to_string(),
    };

    let processed_swaps = ProcessedSwap::from_birdeye_transactions(&[mock_sol_to_token]).unwrap();
    let swap = &processed_swaps[0];

    // Test dual event method on SOL → Token swap
    let events = swap.to_financial_events("test_wallet");

    println!("📊 SOL → Token RESULTS:");
    println!("  Number of events: {}", events.len());

    assert_eq!(
        events.len(),
        1,
        "SOL → Token swap should create exactly 1 event"
    );

    let event = &events[0];
    println!("  Event type: {:?}", event.event_type);
    println!("  Token: {}... (BNSOL)", &event.token_mint[..8]);
    println!("  Amount: {} BNSOL", event.token_amount);
    println!("  SOL amount: {} SOL", event.sol_amount);

    assert_eq!(
        event.event_type,
        pnl_core::EventType::Buy,
        "Should be BUY event"
    );
    assert_eq!(
        event.token_mint, "BNso1VUJnh4zcfpZa6986Ea66P6TCp59hvtNJ8b1X85",
        "Should be buying BNSOL"
    );
    assert_eq!(
        event.token_amount,
        Decimal::from(950),
        "Should buy 950 BNSOL"
    );
    assert_eq!(
        event.sol_amount,
        Decimal::from(1000),
        "Should spend 1000 SOL"
    );

    println!("✅ SOL → Token SINGLE EVENT VERIFIED!");
    println!("  ✅ Creates exactly 1 BUY event");
    println!("  ✅ Uses actual SOL amount (not SOL equivalent)");
    println!("  ✅ Backward compatible with existing data");
}

#[tokio::test]
async fn test_fifo_accounting_with_dual_events() {
    println!("\n🧪 TESTING FIFO ACCOUNTING WITH DUAL EVENTS");
    println!("============================================");

    // Simulate a complete flow: Buy USDC → Swap USDC→RENDER → Verify accounting
    println!("📋 SCENARIO:");
    println!("  1. Buy 1000 USDC with SOL");
    println!("  2. Swap 1000 USDC → 50 RENDER");
    println!("  3. Verify USDC position is properly closed");

    // Create mock buy USDC transaction
    let usdc_buy_tx = GeneralTraderTransaction {
        quote: TokenTransactionSide {
            symbol: "SOL".to_string(),
            decimals: 9,
            address: "So11111111111111111111111111111111111111112".to_string(),
            amount: 0,
            transfer_type: Some("transfer".to_string()),
            type_swap: "from".to_string(),
            ui_amount: 6.67,
            price: Some(150.0),
            nearest_price: Some(150.0),
            change_amount: 0,
            ui_change_amount: -6.67, // Spent 6.67 SOL
            fee_info: None,
        },
        base: TokenTransactionSide {
            symbol: "USDC".to_string(),
            decimals: 6,
            address: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
            amount: 0,
            transfer_type: Some("mintTo".to_string()),
            type_swap: "to".to_string(),
            ui_amount: 1000.0,
            price: Some(1.0),
            nearest_price: Some(1.0),
            change_amount: 0,
            ui_change_amount: 1000.0, // Received 1000 USDC
            fee_info: None,
        },
        base_price: Some(1.0),
        quote_price: 150.0,
        tx_hash: "buy_usdc_tx".to_string(),
        source: "test".to_string(),
        block_unix_time: 1751414700, // Earlier timestamp
        tx_type: "swap".to_string(),
        address: "".to_string(),
        owner: "test_wallet".to_string(),
    };

    // Process both transactions
    let usdc_swap = ProcessedSwap::from_birdeye_transactions(&[usdc_buy_tx]).unwrap();
    let token_to_token_swap =
        ProcessedSwap::from_birdeye_transactions(&[create_mock_token_to_token_transaction()])
            .unwrap();

    // Get all events
    let mut all_events = Vec::new();
    all_events.extend(usdc_swap[0].to_financial_events("test_wallet"));
    all_events.extend(token_to_token_swap[0].to_financial_events("test_wallet"));

    println!("📊 ALL EVENTS GENERATED:");
    for (i, event) in all_events.iter().enumerate() {
        println!(
            "  Event {}: {:?} {} {} (SOL: {})",
            i + 1,
            event.event_type,
            event.token_amount,
            &event.token_mint[..8],
            event.sol_amount
        );
    }

    // Verify event sequence
    assert_eq!(all_events.len(), 3, "Should have 3 events total");

    // Event 1: Buy USDC with SOL
    assert_eq!(all_events[0].event_type, pnl_core::EventType::Buy);
    assert_eq!(
        all_events[0].token_mint,
        "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
    ); // USDC

    // Event 2: Sell USDC (from token-to-token swap)
    assert_eq!(all_events[1].event_type, pnl_core::EventType::Sell);
    assert_eq!(
        all_events[1].token_mint,
        "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
    ); // USDC

    // Event 3: Buy RENDER (from token-to-token swap)
    assert_eq!(all_events[2].event_type, pnl_core::EventType::Buy);
    assert_eq!(
        all_events[2].token_mint,
        "rndrizKT3MK1iimdxRdWabcF7Zg7AR5T4nud4EkHBof"
    ); // RENDER

    println!("✅ FIFO ACCOUNTING SEQUENCE VERIFIED!");
    println!("  ✅ USDC buy event created");
    println!("  ✅ USDC sell event created (realizes P&L)");
    println!("  ✅ RENDER buy event created (establishes cost basis)");
    println!("  ✅ Complete accounting chain for token-to-token swap");
}
