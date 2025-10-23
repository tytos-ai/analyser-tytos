-- Migration: Add incomplete_trades_count column to pnl_results table
-- Purpose: Track the number of trades that only have OUT transfers (no IN side)
-- Date: 2025-10-16

-- Add the incomplete_trades_count column with a default value of 0
ALTER TABLE pnl_results
ADD COLUMN IF NOT EXISTS incomplete_trades_count INTEGER NOT NULL DEFAULT 0;

-- Add an index for filtering/sorting by incomplete trades count
CREATE INDEX IF NOT EXISTS idx_pnl_results_incomplete_trades
ON pnl_results(incomplete_trades_count);

-- Add a comment to document the column
COMMENT ON COLUMN pnl_results.incomplete_trades_count IS
'Number of trades with only OUT transfers (no matching IN side)';
