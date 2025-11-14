-- Migration: Add transaction fetch metadata columns to pnl_results table
-- Purpose: Track timeframe, transaction limits, and whether limit was hit during analysis
-- Date: 2025-11-05

-- Add timeframe_requested column (e.g., "6m", "1y")
ALTER TABLE pnl_results
ADD COLUMN IF NOT EXISTS timeframe_requested VARCHAR(50);

-- Add transaction_limit_requested column (e.g., 10000)
ALTER TABLE pnl_results
ADD COLUMN IF NOT EXISTS transaction_limit_requested INTEGER;

-- Add transactions_fetched column (actual number fetched)
ALTER TABLE pnl_results
ADD COLUMN IF NOT EXISTS transactions_fetched INTEGER NOT NULL DEFAULT 0;

-- Add was_transaction_limit_hit column (boolean flag)
ALTER TABLE pnl_results
ADD COLUMN IF NOT EXISTS was_transaction_limit_hit BOOLEAN NOT NULL DEFAULT false;

-- Add indexes for filtering/sorting
CREATE INDEX IF NOT EXISTS idx_pnl_results_timeframe_requested
ON pnl_results(timeframe_requested);

CREATE INDEX IF NOT EXISTS idx_pnl_results_transaction_limit_hit
ON pnl_results(was_transaction_limit_hit);

-- Add comments to document the columns
COMMENT ON COLUMN pnl_results.timeframe_requested IS
'Timeframe requested for analysis (e.g., "6m", "1y", null if none specified)';

COMMENT ON COLUMN pnl_results.transaction_limit_requested IS
'Transaction limit requested (e.g., 10000, null if none specified)';

COMMENT ON COLUMN pnl_results.transactions_fetched IS
'Actual number of transactions fetched from Zerion API';

COMMENT ON COLUMN pnl_results.was_transaction_limit_hit IS
'Whether the transaction limit was reached before timeframe exhausted';
