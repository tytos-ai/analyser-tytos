-- Migration: Add roi_percentage column and backfill existing records (SAFE VERSION)
-- Purpose: Optimize /api/results endpoint by avoiding portfolio_json deserialization
-- Date: 2025-10-14
-- SAFE: Uses direct SQL UPDATE with JSON operators to avoid Unicode escape sequence issues

-- Step 1: Add roi_percentage column (fast operation)
ALTER TABLE pnl_results ADD COLUMN IF NOT EXISTS roi_percentage NUMERIC DEFAULT 0;

-- Step 2: Create index for fast sorting by ROI
CREATE INDEX IF NOT EXISTS idx_pnl_results_roi ON pnl_results(roi_percentage);

-- Step 3: Backfill roi_percentage for existing records using direct UPDATE
-- This avoids the Unicode escape sequence issue by using direct JSON operators
\echo 'Starting ROI backfill for existing records...'

UPDATE pnl_results
SET roi_percentage = COALESCE(
    (portfolio_json::jsonb->>'profit_percentage')::NUMERIC,
    0
)
WHERE (roi_percentage = 0 OR roi_percentage IS NULL)
  AND portfolio_json IS NOT NULL
  AND portfolio_json::jsonb ? 'profit_percentage';

\echo 'Backfill completed!'

-- Show stats
\echo '=== ROI Distribution ==='
SELECT
    COUNT(*) as total_records,
    COUNT(CASE WHEN roi_percentage > 100 THEN 1 END) as roi_over_100,
    COUNT(CASE WHEN roi_percentage BETWEEN 0 AND 100 THEN 1 END) as roi_0_to_100,
    COUNT(CASE WHEN roi_percentage < 0 THEN 1 END) as roi_negative,
    ROUND(AVG(CASE WHEN roi_percentage != 0 THEN roi_percentage END), 2) as avg_roi
FROM pnl_results;

-- Verify the migration
SELECT
    COUNT(*) as total_records,
    COUNT(CASE WHEN roi_percentage != 0 THEN 1 END) as records_with_roi,
    ROUND(AVG(roi_percentage), 2) as avg_roi,
    ROUND(MIN(roi_percentage), 2) as min_roi,
    ROUND(MAX(roi_percentage), 2) as max_roi
FROM pnl_results;
