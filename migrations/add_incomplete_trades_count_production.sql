-- Migration: Add incomplete_trades_count column to production database
-- Date: 2025-10-16
-- Purpose: Track the number of incomplete trades (trades with only OUT transfers, no matching IN side)
--          per wallet analysis. This helps identify data quality issues and provides visibility
--          into wallets that may have received tokens via airdrops, transfers, or other means.

-- Add incomplete_trades_count to pnl_results table
ALTER TABLE pnl_results
ADD COLUMN IF NOT EXISTS incomplete_trades_count INTEGER DEFAULT 0;

-- Add incomplete_trades_count to batch_results table
ALTER TABLE batch_results
ADD COLUMN IF NOT EXISTS incomplete_trades_count INTEGER DEFAULT 0;

-- Create indices for efficient querying
CREATE INDEX IF NOT EXISTS idx_pnl_incomplete_trades
ON pnl_results(incomplete_trades_count);

CREATE INDEX IF NOT EXISTS idx_batch_incomplete_trades
ON batch_results(incomplete_trades_count);

-- Add comments to document the column
COMMENT ON COLUMN pnl_results.incomplete_trades_count IS
'Count of trades with only OUT transfers (no matching IN side) for this wallet analysis. Indicates tokens received via airdrops, transfers, or data gaps.';

COMMENT ON COLUMN batch_results.incomplete_trades_count IS
'Count of trades with only OUT transfers (no matching IN side) for this wallet in the batch job. Indicates tokens received via airdrops, transfers, or data gaps.';
