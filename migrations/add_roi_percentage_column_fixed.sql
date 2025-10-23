-- Migration: Add roi_percentage column and backfill existing records
-- Purpose: Optimize /api/results endpoint by avoiding portfolio_json deserialization
-- Date: 2025-10-14
-- FIXED: Handle Unicode escape sequences in JSON data

-- Step 1: Add roi_percentage column (fast operation)
ALTER TABLE pnl_results ADD COLUMN IF NOT EXISTS roi_percentage NUMERIC DEFAULT 0;

-- Step 2: Create index for fast sorting by ROI
CREATE INDEX IF NOT EXISTS idx_pnl_results_roi ON pnl_results(roi_percentage);

-- Step 3: Backfill roi_percentage for existing records
-- This extracts profit_percentage from the existing portfolio_json data
-- The profit_percentage is already calculated by the PnL engine

DO $$
DECLARE
    record_count INTEGER := 0;
    batch_size INTEGER := 100;
    current_offset INTEGER := 0;
    total_records INTEGER;
    wallet_rec RECORD;
    profit_percent NUMERIC;
BEGIN
    -- Get total count
    SELECT COUNT(*) INTO total_records
    FROM pnl_results
    WHERE (roi_percentage = 0 OR roi_percentage IS NULL) AND portfolio_json IS NOT NULL;

    RAISE NOTICE 'Starting backfill for % records...', total_records;

    -- Process in batches to avoid memory issues
    LOOP
        FOR wallet_rec IN
            SELECT wallet_address, chain, portfolio_json::jsonb as pjson
            FROM pnl_results
            WHERE (roi_percentage = 0 OR roi_percentage IS NULL) AND portfolio_json IS NOT NULL
            ORDER BY analyzed_at DESC
            LIMIT batch_size OFFSET current_offset
        LOOP
            -- Extract profit_percentage directly from JSONB (avoids text conversion)
            BEGIN
                -- Use JSON path extraction to get profit_percentage as numeric
                profit_percent := COALESCE(
                    (wallet_rec.pjson->>'profit_percentage')::NUMERIC,
                    0
                );
            EXCEPTION WHEN OTHERS THEN
                -- If any error (including Unicode issues), default to 0
                profit_percent := 0;
                RAISE WARNING 'Failed to extract ROI for wallet % on chain %: %',
                    wallet_rec.wallet_address, wallet_rec.chain, SQLERRM;
            END;

            -- Update the record
            UPDATE pnl_results
            SET roi_percentage = profit_percent
            WHERE wallet_address = wallet_rec.wallet_address
              AND chain = wallet_rec.chain;

            record_count := record_count + 1;

            -- Progress logging every 100 records
            IF record_count % 100 = 0 THEN
                RAISE NOTICE 'Processed % / % records...', record_count, total_records;
            END IF;
        END LOOP;

        -- Exit if no more records
        EXIT WHEN NOT FOUND;
        current_offset := current_offset + batch_size;

        -- Exit if we've processed everything
        EXIT WHEN current_offset >= total_records;
    END LOOP;

    RAISE NOTICE 'Backfill completed! Updated % records with ROI percentage.', record_count;

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
