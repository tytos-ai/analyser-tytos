const WebSocket = require('ws');

console.log('🔍 Testing different WebSocket URLs...\n');

// Test basic WebSocket connection
const ws = new WebSocket('wss://io.dexscreener.com/dex/screener/v5/pairs/h24/1', {
  headers: {
    'Origin': 'https://dexscreener.com',
    'User-Agent': 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/137.0.0.0 Safari/537.36',
    'Sec-WebSocket-Extensions': 'permessage-deflate; client_max_window_bits'
  }
});

ws.on('open', function open() {
  console.log('✅ WebSocket connected successfully');
});

ws.on('message', function message(data) {
  console.log('📨 Received message length:', data.length);
  console.log('📨 Message type:', data.constructor.name);
  
  // Try to parse as JSON
  try {
    const parsed = JSON.parse(data.toString());
    console.log('✅ JSON parsed successfully');
    console.log('🔍 First few keys:', Object.keys(parsed).slice(0, 5));
    if (parsed.pairs && Array.isArray(parsed.pairs)) {
      console.log('📊 Number of pairs:', parsed.pairs.length);
      if (parsed.pairs[0]) {
        console.log('🔸 Sample pair:', {
          chainId: parsed.pairs[0].chainId,
          baseToken: parsed.pairs[0].baseToken?.symbol,
          quoteToken: parsed.pairs[0].quoteToken?.symbol,
          priceUsd: parsed.pairs[0].priceUsd
        });
      }
    }
  } catch (e) {
    console.log('❌ Not JSON, raw data preview:', data.toString().substring(0, 200));
  }
});

ws.on('error', function error(err) {
  console.error('❌ WebSocket error:', err.message);
});

ws.on('close', function close() {
  console.log('🔌 WebSocket connection closed');
  process.exit(0);
});

// Close after 10 seconds
setTimeout(() => {
  console.log('⏰ Timeout reached, closing connection');
  ws.close();
}, 10000);