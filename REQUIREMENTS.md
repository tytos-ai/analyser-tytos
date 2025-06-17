# Requirements Document: Rust-based P&L Calculator and Dex Analyzer (Reflecting Current JS/TS Codebase)

## 1. Introduction

*   **Purpose**: To define the requirements for a high-performance system written in Rust, aiming to replicate and significantly enhance the functionality and speed of an existing Node.js/TypeScript application for calculating Profit & Loss (P&L) for Solana wallets and processing data from Decentralized Exchanges (Dex) for wallet discovery.
*   **Scope**: The system will fetch data from the Solana blockchain and DexScreener, process this data according to logic derived from the current JS/TS codebase, provide configuration options via environment variables (and an API in the Rust version), and support batch and continuous operational modes with a focus on speed and parallel processing.

## 2. Functional Requirements

*   **FR1: Solana P&L Calculation**
    *   FR1.1: The system shall be able to ingest Solana wallet addresses for P&L analysis.
    *   FR1.2: For each wallet, it shall fetch all historical transaction data from the Solana blockchain via a configurable RPC endpoint, doing so efficiently and potentially in parallel for multiple wallets in batch mode.
    *   FR1.3: The system shall parse these transactions to identify relevant financial events (buys, sells, fees), mapping them to P&L rules derived from `src/modules/pnl.ts` and `src/modules/txParser.ts`.
    *   FR1.4: P&L calculations are based on identifying buy and sell transactions. Token prices for P&L calculations (e.g., for current holdings or transfers out) are fetched using the Jupiter API (`lite-api.jup.ag` as used in `src/utils/jprice.ts`). Fees associated with trades are to be accounted for. Specific handling of transfers (in/out) and assets held before the timeframe needs to be based on the logic in `pnl.ts` and `txParser.ts`. (Claude: Detailed P&L rules regarding transfers, pre-existing assets, and specific fee accounting must be inferred from the provided JS/TS code, using the 'Wallet Analyzer Bot' PDF as a guide *only if* the code implements similar logic).
    *   FR1.5: The system should support configurable filters for P&L calculation, based on parameters processed from `.env` in `src/Main.ts`. These include:
        *   `TIMEFRAME_MODE`: Controls how the timeframe is determined (`none`, `general`, `specific`).
        *   `TIMEFRAME_GENERAL`: Predefined timeframe duration (e.g., "1min", "1h", "1d", "1m", "1y").
        *   `TIMEFRAME_SPECIFIC`: Specific start/end timestamp for analysis (e.g., "YYYY-MM-DDTHH:mm:ssZ").
        *   `WALLET_MIN_CAPITAL`: Minimum wallet value (interpreted as SOL value) to qualify for analysis.
        *   `AGGREGATOR_MIN_HOLD_MINUTES`: Minimum average holding period for assets.
        *   `MAX_SIGNATURE`: Maximum number of transactions to process per wallet.
        *   `AMOUNT_TRADES`: Minimum number of trades for a wallet to be analyzed.
        *   `WIN_RATE`: Minimum win rate percentage (0-100) for a wallet's trades.

*   **FR2: DexScreener Monitoring & Wallet Processing for Analysis**
    *   FR2.1: The system's Dex component (derived from `Dex/*` scripts) connects to the **DexScreener WebSocket API** (`wss://io.dexscreener.com/dex/screener/v5/pairs/h24/1`) to get a stream of trending pairs. This process must be resilient.
    *   FR2.2: It extracts pair addresses from the WebSocket stream and stores new, unique pairs in Redis (e.g., under `trending:{pair}` with `extracted: false`).
    *   FR2.3: A separate process in the Dex component (derived from `fetchDex.js`, `workerTrending.js`) fetches detailed data for these new pairs from the **DexScreener HTTP API** (`io.dexscreener.com/dex/log/amm/v4/pumpfundex/top/solana/{pair}`), utilizing efficient, potentially concurrent, fetching strategies.
    *   FR2.4: The Dex component then processes this detailed pair data to identify and extract relevant Solana wallet addresses. These discovered wallet addresses are then written to a specific Redis list/key to be picked up by the P&L analysis pipeline. (Claude: The exact logic for wallet extraction from DexScreener pair data must be replicated from `extractSolKeys.js` and `workerTrending.js`).

*   **FR3: Data Input & Sources**
    *   FR3.1: Wallet lists for Batch Analysis Mode can be provided from `wallets.txt` (as read by `src/modules/wallet_read.ts`) or dynamically from Redis (in 24/7 mode, also via `wallet_read.ts`). The Rust rewrite should support API-based list input for this mode.
    *   FR3.2: Solana RPC endpoint is configurable via `SOLANA_RPC_URL`.
    *   FR3.3: The primary Dex data source is DexScreener (WebSocket and HTTP API). URLs are currently hardcoded in `Dex/trendingWs.js` and `Dex/fetchDex.js` but should be made configurable in the Rust version. Token pricing for P&L uses the Jupiter API (`lite-api.jup.ag`) as implemented in `src/utils/jprice.ts`.

*   **FR4: Data Output & Export**
    *   FR4.1: P&L calculation results are exported to CSV format by `src/modules/exporter.ts`. The Rust implementation should replicate the columns and data format produced by `exporter.ts`. (Claude: Analyze `exporter.ts` to determine the exact CSV structure, including sort order).
    *   FR4.2: The Rust system should provide an API endpoint to retrieve P&L results. Dex trending data exposure via API is secondary to P&L results.

*   **FR5: System Configuration**
    *   FR5.1: Key operational parameters are configurable via environment variables (loaded by `dotenv` in `src/Main.ts`). The Rust rewrite will expose these via an API and load them from a configuration file or environment variables.
    *   FR5.2: Configurable parameters include: `SOLANA_RPC_URL`, `REDIS_URL`, `REDIS_MODE` (controls 24/7 operation vs. one-shot), `PROCESS_LOOP_MS` (loop interval in 24/7 mode), `DEBUG_MODE`, and all P&L filter parameters listed in FR1.5. DexScreener URLs, currently hardcoded, should be made configurable in the Rust version.

*   **FR6: API Endpoints (High-Level for Rust Rewrite)**
    *   FR6.1: Endpoints for initiating Batch Analysis P&L calculation runs.
    *   FR6.2: Endpoints for managing and viewing status of the DexScreener monitoring and wallet discovery process.
    *   FR6.3: Endpoints for system configuration (view and update settings).
    *   FR6.4: Endpoints for retrieving P&L results.
    *   FR6.5: Endpoints for checking system status.

*   **FR7: Data Persistence & Caching**
    *   FR7.1: The system shall use Redis for:
        *   Storing `trending:{pair}` keys with `extracted: false/true` status by the Dex component.
        *   Storing lists of discovered wallet addresses pushed by the Dex component for P&L processing.
        *   The `aggregator-lock` key is used to ensure only one instance of the P&L processing loop (`src/Main.ts` in 24/7 mode) is active.
        *   (Claude: Confirm other uses of Redis from the JS code, e.g., caching prices, temporary P&L data like `accamounts`, `temptxids`).

*   **FR8: Operational Modes**
    *   **FR8.1: Mode 1 - Batch Analysis Mode:**
        *   The system supports a batch analysis mode. In the current JS, this is typically triggered by running `src/Main.ts`. If `REDIS_MODE` is not '1' (or not set), it attempts a one-shot run using wallets from `wallets.txt` (via `src/modules/wallet_read.ts`).
        *   The Rust rewrite should offer an API endpoint to submit a list of wallets for batch processing, replicating the one-shot analysis capability and aiming for parallel processing of these wallets.
    *   **FR8.2: Mode 2 - Continuous Integrated Analysis Mode (24/7 Mode):**
        *   The system supports an integrated continuous mode. The `Dex/*` scripts run as a separate process (or group of processes, e.g., `Dex/Main.js` orchestrating `trendingWs.js` and `workerTrending.js`). These scripts use DexScreener to discover wallets and push them to a Redis list.
        *   The P&L analysis part (`src/Main.ts`), when `REDIS_MODE` is '1', operates in a loop (`runStartFlow` in `Main.ts`). In this loop, `getWalletsAndBalances` (from `wallet_read.ts`) fetches wallets from the Redis list populated by the Dex component. It then processes these wallets for P&L.
        *   The Rust rewrite will replicate this: a `dex_client` module continuously discovers wallets and puts them into a Redis queue. A `pnl_processor` module (likely managed by `job_orchestrator`) continuously reads from this Redis queue and performs P&L analysis.
        *   The `aggregator-lock` in Redis is used by `src/Main.ts` to ensure only one instance of its P&L processing loop is active in this mode. The Rust version should implement a similar locking mechanism for its continuous P&L processing loop, while internal steps for each wallet analysis should still be optimized for async performance.

## 3. Non-Functional Requirements

*   **NFR1: Performance:** The Rust rewrite shall aim for **significant improvements in processing speed and resource efficiency** (transaction fetching, P&L calculation, Dex data processing) compared to the current Node.js implementation. This is a primary driver for the rewrite.
*   **NFR2: Parallelism & Concurrency:** The Rust system **must be designed to maximize parallelism and concurrency** using asynchronous programming (e.g., Tokio). This includes:
    *   Concurrent fetching of external data (e.g., Solana transactions for multiple wallets in Batch Mode; multiple DexScreener HTTP API calls if applicable for `dex_client`).
    *   Parallel processing of multiple wallets in Batch Mode.
    *   Efficient asynchronous handling of individual wallet P&L calculation steps (e.g., fetching all its transactions, then processing them).
*   **NFR3: Reliability & Availability:** The system, especially its Continuous Analysis Mode, should be designed for robust 24/7 operation, with graceful error handling for external API failures (Solana RPC, DexScreener, Jupiter) and internal errors.
*   **NFR4: Configurability:** As detailed in FR5, key operational parameters must be configurable, allowing adaptation to different environments and analysis needs.
*   **NFR5: Maintainability:** The Rust codebase should be well-structured into logical modules/crates, following Rust best practices, to enhance long-term maintainability and extensibility.
*   **NFR6: Logging:** Implement comprehensive and structured logging for diagnostics, performance monitoring, and operational flow visibility.

## 4. Future Considerations (Optional - For Rust Rewrite, Beyond Current JS Scope)

*   Support for other Dex data aggregators beyond DexScreener.
*   More advanced P&L reporting features (e.g., tax-lot accounting if requirements evolve).
*   Enhanced scalability solutions (e.g., distributed workers for P&L, though initial focus is on single-instance robustness and performance).
*   A web-based UI for interaction (as detailed in `frontend_description.md`).

This document focuses on replicating and optimizing the existing JS/TS functionalities in Rust. Features not explicitly found in the JS/TS codebase are generally deferred to "Future Considerations" unless specified as a direct enhancement for the Rust version (like API-based configuration).
