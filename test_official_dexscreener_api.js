const https = require('https');

async function testEndpoint(url, description) {
  console.log(`\n🔍 Testing: ${description}`);
  console.log(`🌐 URL: ${url}\n`);
  
  return new Promise((resolve, reject) => {
    const req = https.get(url, {
      headers: {
        'User-Agent': 'Mozilla/5.0 (X11; Linux x86_64) AppleWebSocket/537.36',
        'Accept': '*/*'
      }
    }, (res) => {
      let data = '';
      
      console.log(`📊 Status: ${res.statusCode}`);
      console.log(`📋 Headers:`, Object.keys(res.headers).join(', '));
      
      res.on('data', (chunk) => data += chunk);
      res.on('end', () => {
        try {
          const parsed = JSON.parse(data);
          console.log(`✅ Valid JSON response`);
          
          // Analyze different response structures
          if (Array.isArray(parsed)) {
            console.log(`📊 Array response with ${parsed.length} items`);
            if (parsed[0]) {
              console.log(`🔸 Sample item structure:`, {
                keys: Object.keys(parsed[0]),
                hasChainId: !!parsed[0].chainId,
                hasTokenAddress: !!parsed[0].tokenAddress,
                hasAmount: !!parsed[0].amount,
                hasDescription: !!parsed[0].description
              });
              
              // For token boost responses
              if (parsed[0].tokenAddress) {
                console.log(`💰 Token info:`, {
                  chainId: parsed[0].chainId,
                  tokenAddress: parsed[0].tokenAddress,
                  amount: parsed[0].amount,
                  totalAmount: parsed[0].totalAmount
                });
              }
              
              // For pair responses
              if (parsed[0].pairAddress) {
                console.log(`📈 Pair info:`, {
                  chainId: parsed[0].chainId,
                  dexId: parsed[0].dexId,
                  pairAddress: parsed[0].pairAddress,
                  baseToken: parsed[0].baseToken?.symbol,
                  quoteToken: parsed[0].quoteToken?.symbol,
                  volume24h: parsed[0].volume?.h24,
                  priceChange24h: parsed[0].priceChange?.h24
                });
              }
            }
          } else if (parsed.pairs && Array.isArray(parsed.pairs)) {
            console.log(`📊 Pairs response with ${parsed.pairs.length} pairs`);
            if (parsed.pairs[0]) {
              const sample = parsed.pairs[0];
              console.log(`🔸 Sample pair:`, {
                chainId: sample.chainId,
                dexId: sample.dexId,
                baseToken: sample.baseToken?.symbol,
                quoteToken: sample.quoteToken?.symbol,
                priceUsd: sample.priceUsd,
                volume24h: sample.volume?.h24,
                txns24h: sample.txns?.h24?.buys + sample.txns?.h24?.sells || 0,
                liquidity: sample.liquidity?.usd,
                priceChange24h: sample.priceChange?.h24
              });
            }
          } else {
            console.log(`🔸 Object response structure:`, Object.keys(parsed));
          }
          
          resolve({ success: true, data: parsed });
        } catch (e) {
          console.log(`❌ Invalid JSON:`, data.substring(0, 300));
          resolve({ success: false, error: e.message, data: data.substring(0, 300) });
        }
      });
    });
    
    req.on('error', (err) => {
      console.log(`❌ Request error:`, err.message);
      resolve({ success: false, error: err.message });
    });
    
    req.setTimeout(15000, () => {
      console.log(`⏰ Request timeout`);
      req.destroy();
      resolve({ success: false, error: 'timeout' });
    });
  });
}

async function runOfficialAPITests() {
  console.log('🚀 Starting Official DexScreener API Tests\n');
  
  const tests = [
    // Token Profiles & Boosts
    {
      url: 'https://api.dexscreener.com/token-profiles/latest/v1',
      description: 'Get latest token profiles'
    },
    {
      url: 'https://api.dexscreener.com/token-boosts/latest/v1',
      description: 'Get latest boosted tokens'
    },
    {
      url: 'https://api.dexscreener.com/token-boosts/top/v1',
      description: 'Get tokens with most active boosts'
    },
    
    // Orders (for specific tokens)
    {
      url: 'https://api.dexscreener.com/orders/v1/solana/So11111111111111111111111111111111111111112',
      description: 'Check orders for SOL token'
    },
    
    // New endpoint structure tests
    {
      url: 'https://api.dexscreener.com/tokens/v1/solana/So11111111111111111111111111111111111111112',
      description: 'Get SOL token pairs (new v1 endpoint)'
    },
    {
      url: 'https://api.dexscreener.com/token-pairs/v1/solana/So11111111111111111111111111111111111111112',
      description: 'Get SOL token pools (new v1 endpoint)'
    },
    
    // Test multiple tokens
    {
      url: 'https://api.dexscreener.com/tokens/v1/solana/So11111111111111111111111111111111111111112,EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v',
      description: 'Get multiple tokens (SOL, USDC)'
    }
  ];
  
  const results = [];
  
  for (const test of tests) {
    const result = await testEndpoint(test.url, test.description);
    results.push({ ...test, ...result });
    console.log(`\n${'='.repeat(80)}`);
    
    // Add delay to respect rate limits
    await new Promise(resolve => setTimeout(resolve, 1200)); // 50 requests/minute = 1.2s between requests
  }
  
  // Summary
  console.log('\n📋 SUMMARY OF RESULTS:');
  console.log(`✅ Successful: ${results.filter(r => r.success).length}`);
  console.log(`❌ Failed: ${results.filter(r => !r.success).length}`);
  
  const workingEndpoints = results.filter(r => r.success);
  if (workingEndpoints.length > 0) {
    console.log('\n🎯 WORKING ENDPOINTS FOR TRENDING DATA:');
    workingEndpoints.forEach(endpoint => {
      console.log(`  ✓ ${endpoint.description}`);
    });
  }
  
  console.log('\n✅ All tests completed');
}

runOfficialAPITests().catch(console.error);