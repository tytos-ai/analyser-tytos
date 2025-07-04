#!/usr/bin/env python3
"""
Test script to check current BNSOL prices from Jupiter API
to understand why the price enhancement step is causing massive unrealized P&L.
"""

import requests
import json

def check_jupiter_price():
    """Check current BNSOL price from Jupiter API"""
    
    # BNSOL mint address
    bnsol_mint = "BNso1VUJnh4zcfpZa6986Ea66P6TCp59hvtNJ8b1X85"
    
    # Jupiter price API endpoint
    url = f"https://price.jup.ag/v6/price?ids={bnsol_mint}"
    
    try:
        response = requests.get(url)
        response.raise_for_status()
        
        data = response.json()
        print("🔍 JUPITER PRICE API RESPONSE:")
        print("=" * 50)
        print(json.dumps(data, indent=2))
        
        if 'data' in data and bnsol_mint in data['data']:
            price = data['data'][bnsol_mint]['price']
            print(f"\n💰 Current BNSOL Price: ${price:.2f}")
            
            # Compare with historical prices from our transaction data
            print(f"\n📊 PRICE COMPARISON:")
            print(f"Current BNSOL price (Jupiter): ${price:.2f}")
            print(f"Historical BNSOL price (from transactions): ~$155-175")
            print(f"Price difference: {price / 160:.2f}x")
            
            if price > 200:
                print(f"\n⚠️  WARNING: Current price ${price:.2f} seems unusually high!")
                print("This could explain the massive unrealized P&L inflation.")
        else:
            print("❌ No price data found for BNSOL")
            
    except Exception as e:
        print(f"❌ Error fetching price: {e}")

def main():
    print("🧪 TESTING CURRENT PRICE ENHANCEMENT ISSUE")
    print("=" * 60)
    
    check_jupiter_price()
    
    print("\n🎯 ANALYSIS:")
    print("If Jupiter reports an inflated current price for BNSOL,")
    print("our price enhancement step will multiply all holdings by this")
    print("inflated price, causing massive unrealized P&L calculations.")
    print("\nThis explains the discrepancy between:")
    print("- Manual calculation: -$10.6M (using transaction prices)")
    print("- Our system after enhancement: +$834M (using current inflated price)")

if __name__ == "__main__":
    main()