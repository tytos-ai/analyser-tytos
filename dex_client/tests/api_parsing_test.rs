use dex_client::{BirdEyeClient, BirdEyeConfig, TrendingTokenFilter};
use dex_client::birdeye_client::{TrendingTokenResponse};

#[tokio::test]
async fn test_birdeye_config_creation() {
    let config = BirdEyeConfig::default();
    assert_eq!(config.api_base_url, "https://public-api.birdeye.so");
    assert_eq!(config.request_timeout_seconds, 30);
    
    // Test creating a client
    let client = BirdEyeClient::new(config);
    assert!(client.is_ok());
}

#[test]
fn test_trending_token_filter_defaults() {
    let filter = TrendingTokenFilter::default();
    assert_eq!(filter.min_volume_usd, Some(10000.0));
    assert_eq!(filter.min_price_change_24h, Some(5.0));
    assert_eq!(filter.min_liquidity, Some(50000.0));
    assert_eq!(filter.min_market_cap, Some(100000.0));
    assert_eq!(filter.max_rank, Some(1000));
    assert_eq!(filter.max_tokens, Some(50));
}

#[test]
fn test_birdeye_response_parsing() {
    // Test with the exact JSON response format that was causing issues
    let json_response = r#"
    {
      "data": {
        "tokens": [
          {
            "address": "71Jvq4Epe2FCJ7JFSF7jLXdNk1Wy4Bhqd9iL6bEFELvg",
            "decimals": 6,
            "liquidity": 1577974.0800243,
            "logoURI": "https://upward-sport-headed.quicknode-ipfs.com/ipfs/QmQAtMVZPjsfaTJM9QErBdVqHi1bD9FCBZDr5uzGV96zhU",
            "name": "Gorbagana",
            "symbol": "GOR",
            "volume24hUSD": 54906469.25131502,
            "volume24hChangePercent": null,
            "fdv": 9791356.46665484,
            "marketcap": 9791356.46665484,
            "rank": 1,
            "price": 0.009791464967539111,
            "price24hChangePercent": 39311.748887995556
          }
        ],
        "total": 1000
      },
      "success": true
    }
    "#;
    
    let result = serde_json::from_str::<TrendingTokenResponse>(json_response);
    
    match result {
        Ok(response) => {
            println!("✅ Successfully parsed BirdEye response!");
            assert!(response.success);
            assert_eq!(response.data.total, Some(1000));
            assert_eq!(response.data.tokens.len(), 1);
            
            let token = &response.data.tokens[0];
            assert_eq!(token.address, "71Jvq4Epe2FCJ7JFSF7jLXdNk1Wy4Bhqd9iL6bEFELvg");
            assert_eq!(token.symbol, "GOR");
            assert_eq!(token.name, "Gorbagana");
            assert_eq!(token.decimals, Some(6));
            assert!((token.price - 0.009791464967539111).abs() < f64::EPSILON);
            assert!(token.price_change_24h.is_some());
            let change = token.price_change_24h.unwrap();
            assert!((change - 39311.748887995556).abs() < 1e-10);
            assert_eq!(token.volume_24h, Some(54906469.25131502));
            assert_eq!(token.volume_change_24h, None); // This was null in the response
            assert_eq!(token.liquidity, Some(1577974.0800243));
            assert_eq!(token.fdv, Some(9791356.46665484));
            assert_eq!(token.marketcap, Some(9791356.46665484));
            assert_eq!(token.rank, Some(1));
            assert!(token.logo_uri.is_some());
        }
        Err(e) => {
            panic!("❌ Failed to parse BirdEye response: {}", e);
        }
    }
}