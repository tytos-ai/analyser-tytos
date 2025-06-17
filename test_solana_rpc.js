const https = require('https');

// Test if we can get transaction data for trending pairs
async function testSolanaRPC() {
  console.log('🔍 Testing Solana RPC for Wallet Discovery\n');

  // Using a trending pair address from our analysis
  const trendingPairAddress = 'A7Z2aTBCcBrEmWFrP2jCpzKdiwHAJhdbWiuXdqjyuyew'; // STOPWAR/SOL
  
  console.log(`📈 Testing with trending pair: ${trendingPairAddress}`);
  console.log('   Token: STOPWAR/SOL');
  console.log('   Volume: $2.4M/24h');
  console.log('   Transactions: 82,699/24h\n');

  // Test with public Solana RPC endpoint
  const rpcEndpoints = [
    'https://api.mainnet-beta.solana.com',
    'https://solana-api.projectserum.com',
    'https://rpc.ankr.com/solana'
  ];

  for (const endpoint of rpcEndpoints) {
    console.log(`🌐 Testing RPC endpoint: ${endpoint}`);
    
    try {
      const result = await testGetSignatures(endpoint, trendingPairAddress);
      if (result.success) {
        console.log(`✅ Success! Found ${result.signatures.length} recent signatures`);
        
        if (result.signatures.length > 0) {
          console.log(`📝 Sample signatures:`);
          result.signatures.slice(0, 3).forEach((sig, i) => {
            console.log(`   ${i + 1}. ${sig.signature.substring(0, 20)}...`);
            console.log(`      Slot: ${sig.slot}`);
            console.log(`      Block time: ${new Date(sig.blockTime * 1000).toISOString()}`);
            console.log(`      Status: ${sig.err ? 'Failed' : 'Success'}`);
          });
        }
        
        // Test getting transaction details
        if (result.signatures.length > 0) {
          const sampleTxId = result.signatures[0].signature;
          console.log(`\n🔍 Testing transaction details for: ${sampleTxId.substring(0, 20)}...`);
          
          const txResult = await testGetTransaction(endpoint, sampleTxId);
          if (txResult.success) {
            console.log(`✅ Transaction details retrieved successfully`);
            console.log(`   Accounts involved: ${txResult.accountKeys.length}`);
            console.log(`   Instructions: ${txResult.instructions.length}`);
            
            // Show sample account keys (potential wallet addresses)
            console.log(`\n👛 Sample Account Keys (Potential Wallets):`);
            txResult.accountKeys.slice(0, 5).forEach((key, i) => {
              console.log(`   ${i + 1}. ${key.substring(0, 20)}...`);
            });
          } else {
            console.log(`❌ Failed to get transaction details: ${txResult.error}`);
          }
        }
        
        break; // Success with this endpoint
      } else {
        console.log(`❌ Failed: ${result.error}`);
      }
    } catch (error) {
      console.log(`❌ Error: ${error.message}`);
    }
    
    console.log(''); // Empty line between tests
  }
}

async function testGetSignatures(rpcEndpoint, address) {
  const payload = {
    jsonrpc: '2.0',
    id: 1,
    method: 'getSignaturesForAddress',
    params: [
      address,
      {
        limit: 10 // Get last 10 transactions
      }
    ]
  };

  return new Promise((resolve) => {
    const data = JSON.stringify(payload);
    
    const options = {
      hostname: rpcEndpoint.replace('https://', ''),
      port: 443,
      path: '/',
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Content-Length': data.length
      }
    };

    const req = https.request(options, (res) => {
      let body = '';
      res.on('data', (chunk) => body += chunk);
      res.on('end', () => {
        try {
          const response = JSON.parse(body);
          if (response.error) {
            resolve({ success: false, error: response.error.message });
          } else {
            resolve({ success: true, signatures: response.result });
          }
        } catch (e) {
          resolve({ success: false, error: e.message });
        }
      });
    });

    req.on('error', (e) => {
      resolve({ success: false, error: e.message });
    });

    req.setTimeout(10000, () => {
      req.destroy();
      resolve({ success: false, error: 'timeout' });
    });

    req.write(data);
    req.end();
  });
}

async function testGetTransaction(rpcEndpoint, signature) {
  const payload = {
    jsonrpc: '2.0',
    id: 1,
    method: 'getTransaction',
    params: [
      signature,
      {
        encoding: 'json',
        maxSupportedTransactionVersion: 0
      }
    ]
  };

  return new Promise((resolve) => {
    const data = JSON.stringify(payload);
    
    const options = {
      hostname: rpcEndpoint.replace('https://', ''),
      port: 443,
      path: '/',
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Content-Length': data.length
      }
    };

    const req = https.request(options, (res) => {
      let body = '';
      res.on('data', (chunk) => body += chunk);
      res.on('end', () => {
        try {
          const response = JSON.parse(body);
          if (response.error) {
            resolve({ success: false, error: response.error.message });
          } else if (response.result) {
            const tx = response.result;
            resolve({ 
              success: true, 
              accountKeys: tx.transaction.message.accountKeys,
              instructions: tx.transaction.message.instructions
            });
          } else {
            resolve({ success: false, error: 'No transaction data' });
          }
        } catch (e) {
          resolve({ success: false, error: e.message });
        }
      });
    });

    req.on('error', (e) => {
      resolve({ success: false, error: e.message });
    });

    req.setTimeout(15000, () => {
      req.destroy();
      resolve({ success: false, error: 'timeout' });
    });

    req.write(data);
    req.end();
  });
}

testSolanaRPC().catch(console.error);