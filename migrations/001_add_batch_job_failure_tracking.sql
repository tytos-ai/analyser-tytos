-- Migration: Add failure tracking to batch_jobs table
-- Created: 2025-10-02
-- Description: Adds successful_wallets, failed_wallets, and error_summary columns
--              to track partial failures in batch jobs

-- Add new columns with defaults for backward compatibility
ALTER TABLE batch_jobs
ADD COLUMN IF NOT EXISTS successful_wallets TEXT DEFAULT '[]',
ADD COLUMN IF NOT EXISTS failed_wallets TEXT DEFAULT '[]',
ADD COLUMN IF NOT EXISTS error_summary TEXT DEFAULT NULL;

-- Add comments for documentation
COMMENT ON COLUMN batch_jobs.successful_wallets IS 'JSON array of wallet addresses that were successfully processed';
COMMENT ON COLUMN batch_jobs.failed_wallets IS 'JSON array of wallet addresses that failed processing';
COMMENT ON COLUMN batch_jobs.error_summary IS 'Summary of errors if any wallets failed (e.g., "5 of 10 wallets failed to process")';
