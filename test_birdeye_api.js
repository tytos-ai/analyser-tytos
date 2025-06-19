#!/usr/bin/env node

// Test script to understand BirdEye API endpoints and their responses
const fetch = require('node-fetch');

const BIRDEYE_API_KEY = '5ff313b239ac42e297b830b10ea1871d';
const BASE_URL = 'https://public-api.birdeye.so';

async function testBirdEyeAPI() {
    console.log('🚀 Testing BirdEye API endpoints...\n');

    const headers = {
        'X-API-KEY': BIRDEYE_API_KEY,
        'Content-Type': 'application/json'
    };

    // Test 1: Get trending tokens
    console.log('📈 1. Testing Trending Tokens API');
    try {
        const trendingResponse = await fetch(`${BASE_URL}/defi/token_trending?chain=solana`, {
            headers
        });
        
        if (trendingResponse.ok) {
            const trendingData = await trendingResponse.json();
            console.log('✅ Trending tokens response structure:');
            console.log('Keys:', Object.keys(trendingData));
            
            if (trendingData.data && trendingData.data.tokens) {
                console.log('Sample trending token:', JSON.stringify(trendingData.data.tokens[0], null, 2));
                console.log(`Total trending tokens: ${trendingData.data.tokens.length}`);
            }
        } else {
            console.log('❌ Trending tokens failed:', trendingResponse.status, await trendingResponse.text());
        }
    } catch (error) {
        console.log('❌ Trending tokens error:', error.message);
    }

    console.log('\n' + '='.repeat(80) + '\n');

    // Test 2: Get top traders for a specific token
    console.log('👥 2. Testing Top Traders API');
    
    // Use a popular Solana token (USDC)
    const testTokenAddress = 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v';
    
    try {
        const topTradersResponse = await fetch(`${BASE_URL}/defi/v2/tokens/top_traders?token_address=${testTokenAddress}`, {
            headers
        });
        
        if (topTradersResponse.ok) {
            const topTradersData = await topTradersResponse.json();
            console.log('✅ Top traders response structure:');
            console.log('Keys:', Object.keys(topTradersData));
            
            if (topTradersData.data && topTradersData.data.items) {
                console.log('Sample top trader:', JSON.stringify(topTradersData.data.items[0], null, 2));
                console.log(`Total top traders: ${topTradersData.data.items.length}`);
            }
        } else {
            console.log('❌ Top traders failed:', topTradersResponse.status, await topTradersResponse.text());
        }
    } catch (error) {
        console.log('❌ Top traders error:', error.message);
    }

    console.log('\n' + '='.repeat(80) + '\n');

    // Test 3: Get trader transactions
    console.log('📜 3. Testing Trader Transactions API');
    
    // Use a sample wallet address
    const testWalletAddress = '9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM';
    
    try {
        const traderTxResponse = await fetch(`${BASE_URL}/trader/txs/seek_by_time?wallet=${testWalletAddress}&token_address=${testTokenAddress}&limit=10`, {
            headers
        });
        
        if (traderTxResponse.ok) {
            const traderTxData = await traderTxResponse.json();
            console.log('✅ Trader transactions response structure:');
            console.log('Keys:', Object.keys(traderTxData));
            
            if (traderTxData.data && traderTxData.data.items) {
                console.log('Sample transaction:', JSON.stringify(traderTxData.data.items[0], null, 2));
                console.log(`Total transactions: ${traderTxData.data.items.length}`);
            }
        } else {
            console.log('❌ Trader transactions failed:', traderTxResponse.status, await traderTxResponse.text());
        }
    } catch (error) {
        console.log('❌ Trader transactions error:', error.message);
    }

    console.log('\n' + '='.repeat(80) + '\n');

    // Test 4: Get historical price
    console.log('💰 4. Testing Historical Price API');
    
    // Get price from 1 hour ago
    const oneHourAgo = Math.floor(Date.now() / 1000) - 3600;
    
    try {
        const priceResponse = await fetch(`${BASE_URL}/defi/historical_price_unix?address=${testTokenAddress}&timestamp=${oneHourAgo}`, {
            headers
        });
        
        if (priceResponse.ok) {
            const priceData = await priceResponse.json();
            console.log('✅ Historical price response structure:');
            console.log('Keys:', Object.keys(priceData));
            console.log('Sample price data:', JSON.stringify(priceData, null, 2));
        } else {
            console.log('❌ Historical price failed:', priceResponse.status, await priceResponse.text());
        }
    } catch (error) {
        console.log('❌ Historical price error:', error.message);
    }

    console.log('\n' + '='.repeat(80) + '\n');

    // Test 5: Get current price
    console.log('💵 5. Testing Current Price API');
    
    try {
        const currentPriceResponse = await fetch(`${BASE_URL}/defi/price?address=${testTokenAddress}`, {
            headers
        });
        
        if (currentPriceResponse.ok) {
            const currentPriceData = await currentPriceResponse.json();
            console.log('✅ Current price response structure:');
            console.log('Keys:', Object.keys(currentPriceData));
            console.log('Sample current price data:', JSON.stringify(currentPriceData, null, 2));
        } else {
            console.log('❌ Current price failed:', currentPriceResponse.status, await currentPriceResponse.text());
        }
    } catch (error) {
        console.log('❌ Current price error:', error.message);
    }

    console.log('\n' + '='.repeat(80) + '\n');

    // Test 6: Get multiple prices
    console.log('💰 6. Testing Multi Price API');
    
    const multiTokens = [
        'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v', // USDC
        'So11111111111111111111111111111111111111112'   // SOL
    ];
    
    try {
        const multiPriceResponse = await fetch(`${BASE_URL}/defi/multi_price?list_address=${multiTokens.join(',')}`, {
            headers
        });
        
        if (multiPriceResponse.ok) {
            const multiPriceData = await multiPriceResponse.json();
            console.log('✅ Multi price response structure:');
            console.log('Keys:', Object.keys(multiPriceData));
            console.log('Sample multi price data:', JSON.stringify(multiPriceData, null, 2));
        } else {
            console.log('❌ Multi price failed:', multiPriceResponse.status, await multiPriceResponse.text());
        }
    } catch (error) {
        console.log('❌ Multi price error:', error.message);
    }

    console.log('\n🎯 BirdEye API testing completed!');
}

// Run the test
testBirdEyeAPI().catch(console.error);