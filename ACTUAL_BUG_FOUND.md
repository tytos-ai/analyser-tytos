# Actual Bug Analysis - Based on Code and Logs

## Summary of Findings

After thorough analysis of the code and logs, I found:

### Zerion Transactions for Solano Token
Total: **12 Zerion transactions**

**BUY Solano (SOL OUT):**
1. tx ce65bbd8: 541,359 tokens, $192.24 SOL spent
2. tx 1d54673a: 2,503,750 tokens, $1928.16 SOL spent
3. tx ffe7ad79: 2,643,985 tokens, $1928.16 SOL spent
4. tx 200767a6: 2,785,793 tokens, $1928.16 SOL spent
5. tx ddb27539: 1,023,973 tokens, $694.48 SOL spent
6. tx 83632799: 140,827 tokens, $9.75 SOL spent
7. tx 47073c3b: 299,088 tokens, $19.55 SOL spent
8. tx dc71e3a2: 131,402 tokens, $9.26 SOL spent
9. tx ce396d1f: 131,420 tokens, $9.27 SOL spent
10. tx 239f0fba (AvQb9vkf): **12,005,534 tokens**, $409.83 SOL spent ⭐

**SELL Solano (SOL IN):**
1. tx 6eca8a81 (261axxdq): **9,498,861 tokens**, $1068.58 SOL received ⭐
2. tx d6d545d5 (36Ws79Xf): **12,708,273 tokens**, $271.24 SOL received ⭐

### Events Created by Parser
- 2 transactions → 2 events each (1 Solano + 1 SOL) = 4 Solano events
- 10 transactions → 4 events each (1 Solano + 3 SOL fees) = 40 Solano events
- **Total Solano events: 12** (should match 12 Zerion transactions)

### PNL Engine Reports
- **20 buy events** for Solano
- Total bought: 44,414,270 tokens
- Total sold: 22,207,135 tokens
- Remaining: 22,207,135 tokens

## THE DISCREPANCY

**Parser creates**: 12 Solano events (10 BUY + 2 SELL)
**PNL engine sees**: 20 BUY events

**Where do the extra 8 events come from?**

### Hypothesis 1: BirdEye Enrichment Duplicating Events
The logs show "Identified skipped transaction for enrichment" messages. Maybe the enrichment process is creating duplicate events.

### Hypothesis 2: Event Processing Bug
Maybe events are being processed twice somewhere in the pipeline between the parser and PNL engine.

### Hypothesis 3: Multiple Token Transfers in Single Transaction
Transaction AvQb9vkf might contain multiple Solano transfers that are all being created as separate events.

## NEXT STEP: Check BirdEye Enrichment

Let me check if the enrichment process is adding events or modifying existing ones.

File to examine: `zerion_client/src/lib.rs` - look for enrichment logic
