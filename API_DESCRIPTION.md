# API Descriptions (P&L Tracker Project - Reflecting JS/TS Base)

## 1. Introduction

This document details APIs relevant to the Rust P&L Tracker and Dex Analyzer, focusing on replicating the current JS/TS system's functionalities. It covers external APIs consumed and internal APIs to be exposed by the Axum server. All internal API payloads are JSON.

## 2. External APIs Consumed

    **2.1. Solana RPC API**
    *   **Purpose:** Primary source for fetching Solana blockchain data (signatures, transactions).
    *   **Base URL:** Configurable via `SOLANA_RPC_URL` (e.g., `https://api.mainnet-beta.solana.com`).
    *   **Key Endpoints:** `getSignaturesForAddress`, `getTransaction` (as previously defined).
    *   **Authentication:** None for public RPCs.
    *   **Rate Limiting:** Handled by client with retries.

    **2.2. DexScreener & Jupiter APIs**
    *   **Purpose:** DexScreener for trending pair discovery and wallet identification; Jupiter for token pricing in P&L calculations.
    *   **DexScreener WebSocket API:**
        *   URL: `wss://io.dexscreener.com/dex/screener/v5/pairs/h24/1` (Hardcoded in current JS, to be configurable in Rust).
        *   Usage: Stream of trending pairs, consumed by `dex_client`.
    *   **DexScreener HTTP API:**
        *   URL: `https://io.dexscreener.com/dex/log/amm/v4/pumpfundex/top/solana/{pair}` (Base URL hardcoded in current JS, to be configurable in Rust).
        *   Usage: Fetching specific pair data for wallet extraction by `dex_client`.
    *   **Jupiter Price API:**
        *   URL: `https://lite-api.jup.ag/price/v2` (Used by `jprice_client` via `src/utils/jprice.ts`).
        *   Usage: Fetching token prices for P&L calculations.
    *   **Authentication & Rate Limiting:** DexScreener and Jupiter APIs are generally public. The system must handle their rate limits and error responses robustly. No API keys are mentioned for these in the JS source.

## 3. Internal APIs Exposed (Axum Server - Rust Rewrite)

    **3.1. Configuration Endpoints**
    *   **Base Path:** `/api/config`
    *   **`GET /api/config`**
        *   Description: Retrieves current system configuration reflecting active `.env` settings and defaults.
        *   Response Body (Example, based on JS `.env` variables):
            ```json
            {
                "solana_rpc_url": "https://api.mainnet-beta.solana.com",
                "redis_url": "redis://127.0.0.1/",
                "redis_mode": "1", // "1" for 24/7 mode, "0" or other for one-shot/batch
                "process_loop_ms": 60000, // From PROCESS_LOOP_MS
                "debug_mode": false, // From DEBUG_MODE
                "pnl_parameters": {
                    "timeframe_mode": "general", // From TIMEFRAME_MODE
                    "timeframe_general": "1m",   // From TIMEFRAME_GENERAL
                    "timeframe_specific": null, // From TIMEFRAME_SPECIFIC, e.g., "2023-01-01T00:00:00Z"
                    "wallet_min_capital_sol": 0.5, // From WALLET_MIN_CAPITAL (in SOL)
                    "aggregator_min_hold_minutes": 30, // From AGGREGATOR_MIN_HOLD_MINUTES
                    "max_signatures_per_wallet": 1000, // From MAX_SIGNATURE
                    "min_trades_for_analysis": 10,   // From AMOUNT_TRADES
                    "win_rate_percent": 60          // From WIN_RATE
                },
                "dexscreener_ws_url": "wss://io.dexscreener.com/dex/screener/v5/pairs/h24/1", // Proposed: make configurable
                "dexscreener_http_base_url": "https://io.dexscreener.com/dex/log/amm/v4/pumpfundex/top/solana/", // Proposed: make configurable
                "jupiter_price_api_url": "https://lite-api.jup.ag/price/v2" // Proposed: make configurable
            }
            ```
    *   **`POST /api/config`**
        *   Description: Updates parts of the system configuration. The Rust backend will update its active configuration (which might influence env vars for a restart or update a live config struct).
        *   Request Body: Subset of the `GET /api/config` structure.
        *   Response: Success/failure message.

    **3.2. P&L Calculation Endpoints (Batch Analysis Mode)**
    *   **Base Path:** `/api/pnl/batch`
    *   **`POST /api/pnl/batch/run`**
        *   Description: Initiates a one-shot P&L calculation run for a list of wallets, similar to running `src/Main.ts` without `REDIS_MODE='1'`.
        *   Request Body:
            ```json
            {
                "wallets": ["WALLET_ADDRESS_1", "WALLET_ADDRESS_2"],
                "configuration_snapshot": { // Optional: Overrides for P&L parameters for this run
                    "timeframe_mode": "specific",
                    "timeframe_specific": "2023-01-01T00:00:00Z",
                    "wallet_min_capital_sol": 1.0
                    // ... other relevant P&L params from FR1.5
                }
            }
            ```
        *   Response: `{ "run_id": "BATCH_UUID_STRING", "status": "queued" }`
    *   **`GET /api/pnl/batch/status/{run_id}`**: As previously defined.
    *   **`GET /api/pnl/batch/results/{run_id}`**: As previously defined.
    *   **`GET /api/pnl/batch/results/{run_id}/export.csv`**: As previously defined (CSV structure to match `exporter.ts`).

    **3.3. Continuous Mode & Discovered Wallet Endpoints**
    *   **Base Path:** `/api/pnl/continuous`
    *   **`GET /api/pnl/continuous/discovered-wallets`**
        *   Description: Retrieves wallets discovered by `dex_client` (via DexScreener) and processed by the P&L pipeline in Continuous (24/7) Mode.
        *   Query Parameters: `limit`, `offset`, `sort_by` (e.g., PNL, discovery time).
        *   Response Body Example:
            ```json
            {
                "wallets": [
                    {
                        "wallet_address": "DISCOVERED_WALLET_1",
                        "last_analyzed_at": "ISO_TIMESTAMP",
                        "discovery_source_info": "DexScreener Pair: PAIR_ADDRESS",
                        "pnl_summary": { /* P&L metrics from analysis */ }
                    }
                ],
                "total_discovered": 50
            }
            ```
    *   **`GET /api/pnl/continuous/discovered-wallets/{wallet_address}/details`**: As previously defined.

    **3.4. Dex Monitoring Service Endpoints (Simplified)**
    *   **Base Path:** `/api/dex`
    *   **`GET /api/dex/status`**
        *   Description: Provides status of the `dex_client` service (DexScreener monitoring).
        *   Response Body Example:
            ```json
            {
                "is_dexscreener_ws_connected": true,
                "last_pair_batch_processed_at": "ISO_TIMESTAMP", // When wallets were last pushed to Redis queue
                "discovered_wallets_in_queue": 15, // Approx. count in Redis queue
                "monitoring_errors_count": 0
            }
            ```
    *   **(Optional) `POST /api/dex/control`**
        *   Description: Allows starting/stopping the `dex_client` background service if this control is desired.
        *   Request Body: `{ "action": "start" }` or `{ "action": "stop" }`.

    **3.5. System Status & Logs (as previously defined)**
    *   `GET /api/status`
    *   `GET /api/logs`

## 4. Authentication & Authorization

*   Recommended for `POST /api/config` and any control endpoints like `/api/dex/control`. API Key (`X-API-Key` header) is a straightforward approach.

This API description is tailored to reflect the existing JS/TS system's core data sources (Solana RPC, DexScreener, Jupiter) and operational logic (batch vs. 24/7 mode with Redis interaction), while structuring it for a new Rust/Axum backend.Okay, the `api_descriptions.md` file has been updated to align with the revised "as-is" `requirements.md` and `architecture.md`.

Key changes made:

1.  **External APIs Consumed (Section 2):**
    *   Section 2.2 was refocused to **"DexScreener & Jupiter APIs"**.
    *   Specifically detailed the DexScreener WebSocket API (`wss://io.dexscreener.com/dex/screener/v5/pairs/h24/1`) and HTTP API (`https://io.dexscreener.com/dex/log/amm/v4/pumpfundex/top/solana/{pair}`). Noted that URLs are hardcoded in JS but should be configurable in Rust.
    *   Detailed the Jupiter Price API (`https://lite-api.jup.ag/price/v2`).
    *   Removed subsections for Birdeye, Solscan, as they are not primary, direct dependencies according to the "as-is" review.
    *   Clarified that no API keys are typically needed for DexScreener or Jupiter based on current understanding.

2.  **Internal APIs Exposed (Section 3):**
    *   **Configuration Endpoints (`/api/config`):**
        *   The example request/response now strictly reflects parameters derivable from the JS `.env` variables: `solana_rpc_url`, `redis_url`, `redis_mode`, `process_loop_ms`, `debug_mode`, and the specific P&L filter parameters (`TIMEFRAME_MODE`, `WALLET_MIN_CAPITAL`, etc.).
        *   Added proposed new configurable fields for the Rust version: `dexscreener_ws_url`, `dexscreener_http_base_url`, `jupiter_price_api_url`.
        *   Removed `continuous_mode_settings.wallet_discovery_criteria` (as specific criteria logic is in `Dex/*` scripts and not detailed as configurable in `.env`).
        *   Removed `external_data_sources` for API keys, as the verified external APIs don't typically use them.
    *   **P&L Batch Endpoints (`/api/pnl/batch`):**
        *   `POST .../run` request body's `configuration_snapshot` now refers to the P&L parameters confirmed from `.env`.
    *   **Continuous Mode Endpoints (`/api/pnl/continuous`):**
        *   `GET .../discovered-wallets` response's `discovery_source_info` now reflects discovery via DexScreener.
    *   **Dex Monitoring Endpoints (`/api/dex`):**
        *   Replaced `POST /api/dex/trigger-scan` and `GET /api/dex/current-monitors` with:
            *   `GET /api/dex/status`: To provide operational status of the `dex_client` (DexScreener monitoring).
            *   Optional `POST /api/dex/control`: To start/stop the `dex_client` service, if such direct control is desired for the Rust rewrite. This better reflects managing a continuous service.

3.  **Consistency:**
    *   All changes aim to ensure the API description aligns strictly with the features and data sources of the existing JS/TS application, forming a clear baseline for the Rust rewrite.

The `api_descriptions.md` file has been overwritten with these modifications.
