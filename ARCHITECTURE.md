# Architecture Document: Rust-based P&L Calculator and Dex Analyzer (As-Is JS/TS Base)

## 1. Introduction

*   **Purpose**: This document outlines the proposed architecture for the Rust-based P&L Calculator and Dex Analyzer system, designed to replicate and enhance the existing JS/TS codebase. It describes major components, their interactions, data flow, and technology choices aligned with `requirements.md`.
*   **Goals**: To create a **high-performance, parallel-processing**, reliable, and maintainable Rust system that mirrors the core functionalities of the original application, with a focus on speed and efficiency.

## 2. System Overview

*   A modular system composed of several Rust crates:
    *   **`pnl_core`**: Core P&L calculation logic and data structures.
    *   **`solana_client`**: Solana blockchain interaction (RPC).
    *   **`dex_client`**: DexScreener interaction (WebSocket/HTTP) for wallet discovery.
    *   **`jprice_client`**: Jupiter API interaction for token pricing.
    *   **`tx_parser`**: Transaction parsing and transformation.
    *   **`persistence_layer`**: Redis interaction for caching, queues, and state.
    *   **`api_server`**: Axum-based HTTP API.
    *   **`config_manager`**: Configuration loading and management.
    *   **`job_orchestrator`**: Manages P&L analysis tasks for both operational modes.
    *   (Optional) **`pnl_tracker_cli`**: Command-line interface.

## 3. Components / Crates

*   **3.1. `pnl_core` (Core Logic Crate)**
    *   Responsibilities:
        *   Implements P&L calculation logic based on `FinancialEvent`s, as per rules in `requirements.md` (FR1.4, derived from `pnl.ts`).
        *   Defines `FinancialEvent`, `PnLReport`.
        *   Uses a price fetching mechanism (e.g., a trait implemented by `jprice_client`) to get token prices from Jupiter API for P&L calculations where needed (e.g., current value of holdings, value at time of transfer-out).
    *   Key Structs/Traits: `FinancialEvent`, `PnLReport`, `PnlEngine`, `PriceFetcher`.
    *   Dependencies: `serde`, potentially `async-trait` if using traits for price fetching.

*   **3.2. `solana_client` (Solana Interaction Crate)**
    *   Responsibilities:
        *   Fetching transaction signatures and detailed transaction data from a Solana RPC endpoint.
        *   Handling RPC request batching and rate limiting efficiently.
    *   Key Functions: `get_signatures_for_address_parallel()`, `get_transactions_parallel()`.
    *   Dependencies: `reqwest`, Solana SDK types, `serde_json`, `tokio`.

*   **3.3. `dex_client` (DexScreener Interaction Crate)**
    *   Responsibilities:
        *   Connects to DexScreener WebSocket (`wss://io.dexscreener.com/dex/screener/v5/pairs/h24/1`) for trending pair streams.
        *   Connects to DexScreener HTTP API (`io.dexscreener.com/dex/log/amm/v4/pumpfundex/top/solana/{pair}`) for specific pair data, designed for efficient and potentially concurrent calls if analyzing multiple pairs.
        *   Analyzes this data to extract/identify relevant Solana wallet addresses (replicating logic from `extractSolKeys.js`, `workerTrending.js`).
        *   Publishes these discovered wallet addresses to a designated Redis list (via `persistence_layer`) for consumption by `job_orchestrator`.
    *   Key Functions/Structs: `monitor_dexscreener_stream()`, `fetch_and_process_pair_data()`.
    *   Dependencies: `reqwest`, `tokio-tungstenite`, `serde_json`, `tokio`.

*   **3.4. `jprice_client` (Jupiter Price Client Module/Crate)**
    *   Responsibilities:
        *   Fetches token prices from the Jupiter API (`lite-api.jup.ag`) efficiently.
        *   Implements caching for these prices using Redis (via `persistence_layer`), mirroring `src/utils/jprice.ts`.
    *   Key Functions: `get_price_for_token()`.
    *   Dependencies: `reqwest`, `serde_json`, `tokio`, `persistence_layer`.
    *   Note: This could be a module within `pnl_core` or a shared utility crate.

*   **3.5. `tx_parser` (Transaction Parsing & Transformation Crate)**
    *   Responsibilities:
        *   Transforms raw Solana transaction data (from `solana_client`) into `FinancialEvent`s usable by `pnl_core`, reflecting `txParser.ts` logic. This process should be optimized for speed.
    *   Key Functions: `parse_solana_transaction_to_financial_events()`.
    *   Dependencies: `pnl_core`, Solana SDK types.

*   **3.6. `persistence_layer` (Data Persistence Crate - Redis Focus)**
    *   Responsibilities: Interacting with Redis for:
        *   Storing `trending:{pair}` keys with `extracted: false/true` status by `dex_client`.
        *   Managing the Redis list used as a queue for discovered wallets from `dex_client` for `job_orchestrator`.
        *   Storing P&L calculation state/temporary data (e.g., `accamounts:*`, `temptxids:*` if this pattern from JS is replicated).
        *   Implementing the `aggregator-lock` mechanism for singleton operation of the P&L processing loop in Continuous Mode.
        *   Caching Jupiter API token prices (for `jprice_client`).
    *   Key Functions: Redis commands via `redis-rs` (e.g., `GET`, `SET`, `LPUSH`, `RPOP`, `SETNX` for lock), ensuring efficient Redis communication.
    *   Dependencies: `redis` (Rust Redis client with Tokio integration).

*   **3.7. `api_server` (Axum Web Server Crate)**
    *   Responsibilities: As previously defined, exposing endpoints for configuration, batch P&L runs, and status/results viewing. Designed for concurrent request handling.
    *   Dependencies: `axum`, `tokio`, `serde`, `config_manager`, `job_orchestrator`, `persistence_layer`.

*   **3.8. `config_manager` (Configuration Module/Crate)**
    *   Responsibilities: Loading configuration from `.toml` files / environment variables, including `SOLANA_RPC_URL`, `REDIS_URL`, `REDIS_MODE`, P&L filters, and DexScreener URLs (which should be made configurable).
    *   Dependencies: `config` crate, `serde`.

*   **3.9. `job_orchestrator` (Job Orchestrator / Pipeline Manager Module)**
    *   Responsibilities:
        *   **Batch Mode:** Manages P&L analysis tasks for wallets submitted via API, aiming for parallel execution of P&L calculations for different wallets.
        *   **Continuous Mode:** Monitors the Redis list populated by `dex_client`. Dequeues wallet addresses. Implements or coordinates with `persistence_layer` for a "recently processed" check if desired for the Rust version. Initiates P&L analysis for eligible wallets, ensuring that while wallet processing from the queue is sequential (due to `aggregator-lock`), the internal steps for each wallet's analysis are performed with high async efficiency.
    *   Dependencies: `tokio`, `persistence_layer`, `solana_client`, `tx_parser`, `pnl_core`, `jprice_client`.

## 4. Data Flow Diagrams

*   **4.1. Batch Analysis Mode Data Flow:**
    1.  API Request (`POST /api/pnl/batch/run` with wallet list) -> `api_server`.
    2.  `api_server` forwards request to `job_orchestrator`.
    3.  `job_orchestrator` queues wallet(s) for processing, potentially handling multiple wallets concurrently.
    4.  For each wallet, `job_orchestrator` triggers P&L pipeline (steps a-d executed with high concurrency for different wallets):
        a.  `solana_client` fetches transactions (async).
        b.  `tx_parser` converts raw transactions to `FinancialEvent`s.
        c.  `pnl_core` calculates P&L, using `jprice_client` (async, uses `persistence_layer` for cache) for token prices.
        d.  `persistence_layer` stores results (async).
    5.  API for results/status.

*   **4.2. DexScreener Wallet Discovery and P&L Processing (Continuous Mode Data Flow):**
    1.  `dex_client` (background async task/process):
        a.  Connects to DexScreener WebSocket (async).
        b.  Stores new/unique pair addresses in Redis (`trending:{pair}`, `extracted:false`) via `persistence_layer` (async).
        c.  Fetches detailed pair data from DexScreener HTTP API (async) for pairs marked `extracted:false`.
        d.  Processes pair data to extract Solana wallet addresses.
        e.  Pushes discovered wallet addresses to Redis list (async) via `persistence_layer`. Marks pair as `extracted:true`.
    2.  `job_orchestrator` (main P&L processing loop for Continuous Mode):
        a.  Attempts to acquire `aggregator-lock` from `persistence_layer` (async). If successful:
        b.  Dequeues wallet address from Redis queue (async).
        c.  (Optional: "recently processed" check).
        d.  Initiates P&L analysis for the wallet (internal steps are async and optimized):
            i.  `solana_client` fetches transactions (async).
            ii. `tx_parser` converts.
            iii. `pnl_core` calculates P&L, using `jprice_client` (async) for prices.
            iv. `persistence_layer` stores results and state (async).
        e.  Loop repeats. Releases lock on termination.
    3.  API endpoints in `api_server` allow querying results.

## 5. Technology Stack

*   **Language:** Rust (latest stable).
*   **Web Framework:** Axum.
*   **Asynchronous Runtime:** Tokio.
*   **HTTP Client:** `reqwest`.
*   **Data Sources:**
    *   Solana RPC (direct).
    *   DexScreener (WebSocket & HTTP API).
    *   Jupiter API (`lite-api.jup.ag`) for token prices.
*   **Serialization:** `serde`, `serde_json`.
*   **Configuration:** `config` crate, `.toml` files / environment variables.
*   **Caching & Queuing:** Redis (`redis-rs` with Tokio integration).
*   **WebSocket Client:** `tokio-tungstenite`.
*   **Logging:** `tracing`.
*   **Error Handling:** `thiserror`, `anyhow`.

## 6. Concurrency and Parallelism Strategy

*   **Tokio Ecosystem:** Central for all async operations, enabling high concurrency for I/O-bound tasks (Solana RPC, DexScreener API, Jupiter API, Redis interactions).
*   **`dex_client`:** Employs async tasks for WebSocket monitoring and HTTP fetching to efficiently gather data from DexScreener.
*   **`job_orchestrator` / P&L Pipeline:**
    *   **Batch Mode:** Designed to process multiple wallets from a single batch request **in parallel**. Each wallet's P&L calculation (including its data fetching and parsing steps) is a separate Tokio task.
    *   **Continuous Mode:** The main P&L processing loop, after acquiring the `aggregator-lock` (ensuring sequential wallet processing *from the queue*), executes all internal steps for a single wallet's analysis (transaction fetching, parsing, price lookups, calculations) using efficient asynchronous operations to maximize throughput for that wallet before moving to the next.
*   **`solana_client` & `jprice_client`:** Utilize `reqwest`'s async capabilities for concurrent external API calls, managed with rate limiting.
*   **Rate Limiting:** Semaphores or similar mechanisms will be used to manage concurrent calls to external services to avoid being rate-limited.

## 7. Error Handling and Logging

*   As previously defined. Use `tracing` for contextual logging, including performance timings for key operations.

## 8. Build, Test, and Deployment

*   As previously defined.

This revised architecture aligns more closely with the functionalities derived from the existing JS/TS codebase, particularly regarding the use of DexScreener, Jupiter API, and Redis patterns like `aggregator-lock`, while emphasizing performance and parallelism in the Rust implementation.
