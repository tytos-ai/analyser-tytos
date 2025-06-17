const https = require('https');

async function testEndpoint(url, description) {
  console.log(`\n🔍 Testing: ${description}`);
  console.log(`🌐 URL: ${url}\n`);
  
  return new Promise((resolve, reject) => {
    const req = https.get(url, {
      headers: {
        'User-Agent': 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36'
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
          
          // Analyze structure
          if (parsed.pairs && Array.isArray(parsed.pairs)) {
            console.log(`📊 Number of pairs: ${parsed.pairs.length}`);
            if (parsed.pairs[0]) {
              const sample = parsed.pairs[0];
              console.log(`🔸 Sample pair structure:`, {
                chainId: sample.chainId,
                dexId: sample.dexId,
                pairAddress: sample.pairAddress,
                baseToken: sample.baseToken?.symbol,
                quoteToken: sample.quoteToken?.symbol,
                priceUsd: sample.priceUsd,
                hasVolume: !!sample.volume,
                hasLiquidity: !!sample.liquidity,
                hasTxns: !!sample.txns
              });
            }
          } else if (parsed.pair) {
            console.log(`🔸 Single pair structure:`, {
              chainId: parsed.pair.chainId,
              dexId: parsed.pair.dexId,
              pairAddress: parsed.pair.pairAddress,
              baseToken: parsed.pair.baseToken?.symbol,
              quoteToken: parsed.pair.quoteToken?.symbol,
              priceUsd: parsed.pair.priceUsd
            });
          } else {
            console.log(`🔸 Response structure:`, Object.keys(parsed));
          }
          
          resolve({ success: true, data: parsed });
        } catch (e) {
          console.log(`❌ Invalid JSON:`, data.substring(0, 200));
          resolve({ success: false, error: e.message, data: data.substring(0, 200) });
        }
      });
    });
    
    req.on('error', (err) => {
      console.log(`❌ Request error:`, err.message);
      resolve({ success: false, error: err.message });
    });
    
    req.setTimeout(10000, () => {
      console.log(`⏰ Request timeout`);
      req.destroy();
      resolve({ success: false, error: 'timeout' });
    });
  });
}

async function runTests() {
  console.log('🚀 Starting DexScreener API Tests\n');
  
  const tests = [
    {
      url: 'https://api.dexscreener.com/latest/dex/tokens/So11111111111111111111111111111111111111112',
      description: 'Get SOL token pairs'
    },
    {
      url: 'https://api.dexscreener.com/latest/dex/search?q=SOL',
      description: 'Search for SOL pairs'
    },
    {
      url: 'https://api.dexscreener.com/latest/dex/pairs/solana/8slbnzoa1cfnvmjlpfp98zlanfsycfapfjkmbixnlwxj',
      description: 'Get specific pair data'
    },
    {
      url: 'https://api.dexscreener.com/latest/dex/search?q=pump',
      description: 'Search for pump tokens'
    }
  ];
  
  for (const test of tests) {
    const result = await testEndpoint(test.url, test.description);
    console.log(`\n${'='.repeat(80)}`);
  }
  
  console.log('\n✅ All tests completed');
}

runTests().catch(console.error);