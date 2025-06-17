const https = require('https');

async function testJupiterEndpoint(url, description) {
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
          if (parsed.data && typeof parsed.data === 'object') {
            console.log(`🔸 Price data structure:`, {
              tokenCount: Object.keys(parsed.data).length,
              sampleTokens: Object.keys(parsed.data).slice(0, 3),
              samplePrice: Object.values(parsed.data)[0]
            });
          } else if (parsed.price) {
            console.log(`🔸 Single price:`, parsed.price);
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

async function runJupiterTests() {
  console.log('🚀 Starting Jupiter API Tests\n');
  
  const tests = [
    {
      url: 'https://price.jup.ag/v6/price?ids=So11111111111111111111111111111111111111112',
      description: 'Get SOL price from Jupiter v6'
    },
    {
      url: 'https://lite-api.jup.ag/price/v2?ids=So11111111111111111111111111111111111111112',
      description: 'Get SOL price from Jupiter lite-api v2'
    },
    {
      url: 'https://price.jup.ag/v6/price?ids=So11111111111111111111111111111111111111112,EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v',
      description: 'Get multiple token prices (SOL, USDC)'
    },
    {
      url: 'https://lite-api.jup.ag/price/v2?ids=So11111111111111111111111111111111111111112,EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v',
      description: 'Get multiple token prices from lite-api'
    }
  ];
  
  for (const test of tests) {
    const result = await testJupiterEndpoint(test.url, test.description);
    console.log(`\n${'='.repeat(80)}`);
  }
  
  console.log('\n✅ All Jupiter tests completed');
}

runJupiterTests().catch(console.error);