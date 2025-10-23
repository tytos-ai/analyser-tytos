-- Migration: Add roi_percentage column and backfill existing records (SKIP BAD RECORDS)
-- Purpose: Optimize /api/results endpoint by avoiding portfolio_json deserialization
-- Date: 2025-10-14
-- SAFE: Processes each record individually and skips any that fail

-- Step 1: Add roi_percentage column (fast operation)
ALTER TABLE pnl_results ADD COLUMN IF NOT EXISTS roi_percentage NUMERIC DEFAULT 0;

-- Step 2: Create index for fast sorting by ROI
CREATE INDEX IF NOT EXISTS idx_pnl_results_roi ON pnl_results(roi_percentage);

-- Step 3: Backfill roi_percentage for existing records, skipping problematic ones
DO $$
DECLARE
    record_count INTEGER := 0;
    success_count INTEGER := 0;
    error_count INTEGER := 0;
    total_records INTEGER;
    wallet_rec RECORD;
    profit_percent NUMERIC;
BEGIN
    -- Get total count
    SELECT COUNT(*) INTO total_records
    FROM pnl_results
    WHERE (roi_percentage = 0 OR roi_percentage IS NULL) AND portfolio_json IS NOT NULL;

    RAISE NOTICE 'Starting backfill for % records...', total_records;

    -- Process each record individually with error handling
    FOR wallet_rec IN
        SELECT wallet_address, chain
        FROM pnl_results
        WHERE (roi_percentage = 0 OR roi_percentage IS NULL) AND portfolio_json IS NOT NULL
        ORDER BY analyzed_at DESC
    LOOP
        BEGIN
            -- Try to extract profit_percentage from JSONB
            SELECT COALESCE(
                (portfolio_json::jsonb->>'profit_percentage')::NUMERIC,
                0
            ) INTO profit_percent
            FROM pnl_results
            WHERE wallet_address = wallet_rec.wallet_address
              AND chain = wallet_rec.chain;

            -- Update the record
            UPDATE pnl_results
            SET roi_percentage = profit_percent
            WHERE wallet_address = wallet_rec.wallet_address
              AND chain = wallet_rec.chain;

            success_count := success_count + 1;

        EXCEPTION WHEN OTHERS THEN
            -- Skip this record if any error occurs (including Unicode issues)
            error_count := error_count + 1;
            IF error_count <= 10 THEN
                RAISE WARNING 'Skipping wallet % on chain %: %',
                    wallet_rec.wallet_address, wallet_rec.chain, SQLERRM;
            END IF;
        END;

        record_count := record_count + 1;

        -- Progress logging every 1000 records
        IF record_count % 1000 = 0 THEN
            RAISE NOTICE 'Processed % / % records (success: %, errors: %)...',
                record_count, total_records, success_count, error_count;
        END IF;
    END LOOP;

    RAISE NOTICE 'Backfill completed!';
    RAISE NOTICE 'Total processed: %', record_count;
    RAISE NOTICE 'Successful updates: %', success_count;
    RAISE NOTICE 'Skipped (errors): %', error_count;

    -- Show some stats
    RAISE NOTICE '=== ROI Distribution ===';
    RAISE NOTICE 'Records with ROI > 100%%: %', (SELECT COUNT(*) FROM pnl_results WHERE roi_percentage > 100);
    RAISE NOTICE 'Records with ROI 0-100%%: %', (SELECT COUNT(*) FROM pnl_results WHERE roi_percentage BETWEEN 0 AND 100);
    RAISE NOTICE 'Records with ROI < 0%%: %', (SELECT COUNT(*) FROM pnl_results WHERE roi_percentage < 0);
    RAISE NOTICE 'Average ROI: %', (SELECT ROUND(AVG(roi_percentage), 2) FROM pnl_results WHERE roi_percentage != 0);
END $$;

-- Verify the migration
SELECT
    COUNT(*) as total_records,
    COUNT(CASE WHEN roi_percentage != 0 THEN 1 END) as records_with_roi,
    ROUND(AVG(roi_percentage), 2) as avg_roi,
    ROUND(MIN(roi_percentage), 2) as min_roi,
    ROUND(MAX(roi_percentage), 2) as max_roi
FROM pnl_results;
