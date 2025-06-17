const https = require('https');

async function fetchData(url) {
  return new Promise((resolve, reject) => {
    const req = https.get(url, {
      headers: {
        'User-Agent': 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36',
        'Accept': '*/*'
      }
    }, (res) => {
      let data = '';
      res.on('data', (chunk) => data += chunk);
      res.on('end', () => {
        try {
          resolve(JSON.parse(data));
        } catch (e) {
          reject(e);
        }
      });
    });
    
    req.on('error', reject);
    req.setTimeout(10000, () => {
      req.destroy();
      reject(new Error('timeout'));
    });
  });
}

async function delay(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

async function analyzeTrendingTokens() {
  console.log('🔍 Analyzing Trending Tokens Strategy\n');

  try {
    // Step 1: Get boosted tokens (these are likely trending/popular)
    console.log('📈 Fetching boosted tokens...');
    const [latestBoosted, topBoosted] = await Promise.all([
      fetchData('https://api.dexscreener.com/token-boosts/latest/v1'),
      fetchData('https://api.dexscreener.com/token-boosts/top/v1')
    ]);

    console.log(`✅ Latest boosted: ${latestBoosted.length} tokens`);
    console.log(`✅ Top boosted: ${topBoosted.length} tokens`);

    // Step 2: Combine and deduplicate token addresses
    const allBoostedTokens = [...latestBoosted, ...topBoosted];
    const uniqueTokens = new Map();
    
    allBoostedTokens.forEach(token => {
      if (token.chainId === 'solana') {
        const key = token.tokenAddress;
        if (!uniqueTokens.has(key)) {
          uniqueTokens.set(key, {
            address: token.tokenAddress,
            totalAmount: token.totalAmount || token.amount || 0,
            description: token.description || '',
            url: token.url || ''
          });
        } else {
          // Update with higher boost amount
          const existing = uniqueTokens.get(key);
          const newAmount = token.totalAmount || token.amount || 0;
          if (newAmount > existing.totalAmount) {
            existing.totalAmount = newAmount;
          }
        }
      }
    });

    console.log(`🎯 Found ${uniqueTokens.size} unique Solana boosted tokens`);

    // Step 3: Get trading data for top boosted tokens (limit to top 10 to respect rate limits)
    const sortedTokens = Array.from(uniqueTokens.values())
      .sort((a, b) => b.totalAmount - a.totalAmount)
      .slice(0, 10);

    console.log('\n📊 Analyzing top 10 boosted tokens:');
    
    const tokenAnalysis = [];
    
    for (let i = 0; i < sortedTokens.length; i++) {
      const token = sortedTokens[i];
      console.log(`\n${i + 1}. Token: ${token.address.substring(0, 8)}...`);
      console.log(`   Boost Amount: ${token.totalAmount}`);
      
      try {
        // Get pairs for this token
        const pairs = await fetchData(`https://api.dexscreener.com/token-pairs/v1/solana/${token.address}`);
        
        if (pairs && pairs.length > 0) {
          // Find the pair with highest volume
          const topPair = pairs.reduce((best, current) => {
            const currentVol = current.volume?.h24 || 0;
            const bestVol = best.volume?.h24 || 0;
            return currentVol > bestVol ? current : best;
          });

          const analysis = {
            tokenAddress: token.address,
            boostAmount: token.totalAmount,
            description: token.description,
            topPair: {
              pairAddress: topPair.pairAddress,
              dexId: topPair.dexId,
              baseToken: topPair.baseToken?.symbol,
              quoteToken: topPair.quoteToken?.symbol,
              priceUsd: parseFloat(topPair.priceUsd || 0),
              volume24h: topPair.volume?.h24 || 0,
              volume6h: topPair.volume?.h6 || 0,
              volume1h: topPair.volume?.h1 || 0,
              txns24h: (topPair.txns?.h24?.buys || 0) + (topPair.txns?.h24?.sells || 0),
              txns6h: (topPair.txns?.h6?.buys || 0) + (topPair.txns?.h6?.sells || 0),
              txns1h: (topPair.txns?.h1?.buys || 0) + (topPair.txns?.h1?.sells || 0),
              priceChange24h: topPair.priceChange?.h24 || 0,
              priceChange6h: topPair.priceChange?.h6 || 0,
              priceChange1h: topPair.priceChange?.h1 || 0,
              liquidity: topPair.liquidity?.usd || 0,
              marketCap: topPair.marketCap || 0,
              createdAt: topPair.pairCreatedAt
            }
          };

          tokenAnalysis.push(analysis);

          console.log(`   📈 Best Pair: ${analysis.topPair.baseToken}/${analysis.topPair.quoteToken}`);
          console.log(`   💰 Price: $${analysis.topPair.priceUsd}`);
          console.log(`   📊 24h Volume: $${analysis.topPair.volume24h.toLocaleString()}`);
          console.log(`   🔄 24h Txns: ${analysis.topPair.txns24h}`);
          console.log(`   📈 24h Change: ${analysis.topPair.priceChange24h}%`);
          console.log(`   💧 Liquidity: $${analysis.topPair.liquidity.toLocaleString()}`);
        } else {
          console.log(`   ❌ No pairs found`);
        }
        
        // Respect rate limits
        await delay(1200);
        
      } catch (error) {
        console.log(`   ❌ Error fetching data: ${error.message}`);
      }
    }

    // Step 4: Analyze patterns and create trending strategy
    console.log('\n🎯 TRENDING TOKEN ANALYSIS SUMMARY:');
    console.log('='.repeat(50));
    
    if (tokenAnalysis.length > 0) {
      // Sort by different metrics to identify trending patterns
      const byVolume = [...tokenAnalysis].sort((a, b) => b.topPair.volume24h - a.topPair.volume24h);
      const byTxns = [...tokenAnalysis].sort((a, b) => b.topPair.txns24h - a.topPair.txns24h);
      const byPriceChange = [...tokenAnalysis].sort((a, b) => b.topPair.priceChange24h - a.topPair.priceChange24h);

      console.log('\n🔥 Top by 24h Volume:');
      byVolume.slice(0, 3).forEach((token, i) => {
        console.log(`${i + 1}. ${token.topPair.baseToken}/${token.topPair.quoteToken} - $${token.topPair.volume24h.toLocaleString()}`);
      });

      console.log('\n⚡ Top by 24h Transactions:');
      byTxns.slice(0, 3).forEach((token, i) => {
        console.log(`${i + 1}. ${token.topPair.baseToken}/${token.topPair.quoteToken} - ${token.topPair.txns24h} txns`);
      });

      console.log('\n📈 Top by 24h Price Change:');
      byPriceChange.slice(0, 3).forEach((token, i) => {
        console.log(`${i + 1}. ${token.topPair.baseToken}/${token.topPair.quoteToken} - ${token.topPair.priceChange24h}%`);
      });

      // Calculate averages for trending threshold
      const avgVolume = tokenAnalysis.reduce((sum, t) => sum + t.topPair.volume24h, 0) / tokenAnalysis.length;
      const avgTxns = tokenAnalysis.reduce((sum, t) => sum + t.topPair.txns24h, 0) / tokenAnalysis.length;
      
      console.log('\n📊 TRENDING THRESHOLDS (based on boosted tokens):');
      console.log(`   • Volume threshold: $${avgVolume.toLocaleString()}`);
      console.log(`   • Transaction threshold: ${Math.round(avgTxns)} txns/24h`);
      console.log(`   • Min liquidity: $${Math.min(...tokenAnalysis.map(t => t.topPair.liquidity)).toLocaleString()}`);
      
      // Find best pairs for wallet discovery
      console.log('\n🎯 BEST PAIRS FOR WALLET DISCOVERY:');
      const bestPairs = tokenAnalysis
        .filter(t => t.topPair.volume24h > avgVolume && t.topPair.txns24h > avgTxns)
        .slice(0, 5);
        
      bestPairs.forEach((token, i) => {
        console.log(`${i + 1}. Pair: ${token.topPair.pairAddress}`);
        console.log(`   Token: ${token.topPair.baseToken}/${token.topPair.quoteToken}`);
        console.log(`   Volume: $${token.topPair.volume24h.toLocaleString()}`);
        console.log(`   Activity: ${token.topPair.txns24h} txns/24h`);
        console.log('');
      });
    }

  } catch (error) {
    console.error('❌ Error in analysis:', error.message);
  }
}

analyzeTrendingTokens().catch(console.error);