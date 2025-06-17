# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository. It consolidates information from various project documents. For more specific Rust coding standards (error handling, logging, testing), please also refer to `ai_coding_guidelines.md`.

## Project Overview

(Derived from `project_description.md` and `requirements.md` intro)

This project is a **Rust rewrite of an existing Node.js/TypeScript P&L (Profit & Loss) tracker and Dex (Decentralized Exchange) analyzer**, specifically for the Solana blockchain. The **primary objectives** for this rewrite are to achieve **significant performance improvements**, enable **robust parallelism** in data fetching and processing, and expose all system functionalities via a **modern, comprehensive HTTP API** built with the Axum framework.

The Rust system will replicate the core functionalities of the current application, which include:

*   **Batch Analysis Mode:** Allows users to submit specific lists of Solana wallet addresses (e.g., from `wallets.txt` or API input) for on-demand P&L calculation.
*   **Continuous Analysis Mode (24/7 Mode):** Proactively monitors **DexScreener data** (WebSocket for trending pairs, HTTP API for specific pair details) to discover potentially interesting trader wallets. These discovered wallets are then queued (via Redis) for P&L analysis by a separate processing loop. This mode is aimed at supporting copy trading research and replicates the existing `Dex/*` scripts feeding the `src/Main.ts` `REDIS_MODE='1'` loop.

The focus is on a faithful translation of existing features and logic (from `Full_sys_tim/` codebase) into a more performant and robust Rust implementation, accessible via the new API layer. Token pricing for P&L calculations is sourced from the **Jupiter API**.

## Architecture

The workspace consists of 8 crates organized by domain, promoting modularity and separation of concerns. This architecture is designed for high-performance and parallel processing. Refer to `architecture.md` for detailed diagrams and data flows.

-   **`pnl_core`**:
    *   **Responsibilities:** Implements core P&L calculation logic based on `FinancialEvent`s, adhering to rules from `requirements.md` (FR1.4, derived from `pnl.ts`). Defines `FinancialEvent`, `PnLReport`. Uses a price fetching mechanism (via `jprice_client`) for Jupiter API prices.
    *   **Key Technologies/Patterns:** Pure Rust logic, `serde`.

-   **`solana_client`**:
    *   **Responsibilities:** Handles direct Solana RPC communication. Fetches transaction signatures and details. Manages RPC batching/rate limiting efficiently.
    *   **Key Technologies/Patterns:** `reqwest`, Solana SDK types, `tokio`.

-   **`dex_client` (DexScreener Interaction Crate)**:
    *   **Responsibilities:** Connects to DexScreener WebSocket for trending pairs and HTTP API for specific pair data. Analyzes this data to extract/identify Solana wallet addresses (replicating logic from `extractSolKeys.js`, `workerTrending.js`). Publishes discovered wallets to a Redis list (via `persistence_layer`).
    *   **Key Technologies/Patterns:** `reqwest`, `tokio-tungstenite`, `serde_json`, `tokio`.

-   **`jprice_client` (Jupiter Price Client)**:
    *   **Responsibilities:** Fetches token prices from Jupiter API (`lite-api.jup.ag`). Implements Redis caching for these prices (via `persistence_layer`), mirroring `src/utils/jprice.ts`.
    *   **Key Technologies/Patterns:** `reqwest`, `serde_json`, `tokio`, `persistence_layer`.

-   **`tx_parser`**:
    *   **Responsibilities:** Transforms raw Solana transaction data (from `solana_client`) into `FinancialEvent`s for `pnl_core`, reflecting `txParser.ts` logic. Optimized for speed.
    *   **Key Technologies/Patterns:** Uses types from `pnl_core`, Solana SDKs.

-   **`persistence_layer` (Redis Focus)**:
    *   **Responsibilities:** Manages Redis interactions for: DexScreener `trending:{pair}` state, wallet queue for Continuous Mode, P&L temp data (`accamounts:*`, `temptxids:*` if replicated), the `aggregator-lock` for singleton P&L loop in Continuous Mode, and Jupiter price caching.
    *   **Key Technologies/Patterns:** `redis-rs` (Tokio integration).

-   **`api_server` (Axum Web Server)**:
    *   **Responsibilities:** Exposes RESTful API (Axum) for configuration, batch P&L runs, status/results. Designed for concurrent requests.
    *   **Key Technologies/Patterns:** Axum, Tokio, `serde_json`.

-   **`config_manager`**:
    *   **Responsibilities:** Loads config from `.toml`/env vars (mapping to JS `.env` params like `SOLANA_RPC_URL`, `REDIS_MODE`, P&L filters, and new configurables like DexScreener/Jupiter URLs). Provides type-safe `SystemConfig`.
    *   **Key Technologies/Patterns:** `config` crate, `serde`.

-   **`job_orchestrator` (Pipeline Manager)**:
    *   **Responsibilities:**
        *   Batch Mode: Manages P&L tasks for API-submitted wallet lists (aiming for parallel execution).
        *   Continuous Mode: Monitors Redis wallet queue (from `dex_client`), acquires `aggregator-lock`, dequeues wallets, and initiates P&L analysis (sequentially from queue due to lock, but internal steps are async/performant).
    *   **Key Technologies/Patterns:** Tokio, interacts with `persistence_layer` and P&L crates.

**Key Data Structures / Concepts (from `architecture.md`):**
*   **`FinancialEvent`**: Represents a parsed transaction relevant to P&L (buy, sell, fee).
*   **`PnLReport`**: Holds calculated P&L results for a wallet.
*   **Data Flow - Batch Mode:** API -> `job_orchestrator` -> (Parallel for each wallet: `solana_client` -> `tx_parser` -> `pnl_core` + `jprice_client`) -> `persistence_layer`.
*   **Data Flow - Continuous Mode:** `dex_client` (DexScreener) -> Redis (`trending:{pair}` & wallet queue) -> `job_orchestrator` (`aggregator-lock`, sequential from queue) -> (`solana_client` -> `tx_parser` -> `pnl_core` + `jprice_client`) -> `persistence_layer`.

## Detailed Functional Requirements

This section summarizes key functional aspects derived from the "as-is" JS/TS codebase and detailed in `requirements.md`. For full details, Claude should refer to `requirements.md`.

**1. P&L Calculation Logic (Ref: `requirements.md` FR1.4)**
   The system must calculate Profit & Loss adhering to the following core rules (logic to be replicated from `src/modules/pnl.ts`, `src/modules/txParser.ts`):
    *   **Scope:** Only token purchases and sales are considered for P&L.
    *   **Pricing:** Token prices for P&L calculations (e.g., for current holdings or transfers out) are fetched using the **Jupiter API** (via `jprice_client`).
    *   **Fees:** Transaction fees directly associated with buys/sells must be accurately accounted for.
    *   **Transfers & Pre-existing Assets:** The specific handling of token transfers (both in and out of the wallet) and assets held *before* the analysis timeframe begins must precisely mirror the logic in the existing JS `pnl.ts` and `txParser.ts`. (Claude: This is a critical area requiring careful analysis of the JS source).
    *   **Timeframe Bound:** Only tokens *bought within* the selected analysis timeframe are included in P&L.

**2. Operational Modes (Ref: `requirements.md` FR8)**
    *   **Batch Analysis Mode (Mode 1):**
        *   Processes a user-provided list of Solana wallet addresses (input via API, initially from `wallets.txt` in JS). Replicates the one-shot analysis capability of `src/Main.ts` when `REDIS_MODE` is not '1'.
        *   The Rust implementation should aim for parallel processing of wallets in a batch.
    *   **Continuous Integrated Analysis Mode (Mode 2 / "24/7 Mode"):**
        *   The `dex_client` (replicating `Dex/*` scripts) uses **DexScreener (WebSocket & HTTP)** to discover wallets and pushes them to a Redis list.
        *   The P&L processor (replicating `src/Main.ts` with `REDIS_MODE='1'`, managed by `job_orchestrator` in Rust) reads from this Redis list.
        *   An `aggregator-lock` in Redis ensures only one instance of this P&L processing loop is active, processing wallets sequentially from the queue (though internal steps for each wallet are async/performant).

**3. Configurable P&L Filter Parameters (Ref: `requirements.md` FR1.5, from JS `.env`)**
   The following parameters, loaded by `config_manager`, must be filterable/configurable for P&L analysis:
    *   `TIMEFRAME_MODE` (`none`, `general`, `specific`)
    *   `TIMEFRAME_GENERAL` (e.g., "1m", "1y")
    *   `TIMEFRAME_SPECIFIC` (e.g., "YYYY-MM-DDTHH:mm:ssZ")
    *   `WALLET_MIN_CAPITAL` (in SOL)
    *   `AGGREGATOR_MIN_HOLD_MINUTES`
    *   `MAX_SIGNATURE` (transaction limit per wallet)
    *   `AMOUNT_TRADES` (minimum trades)
    *   `WIN_RATE` (percentage 0-100)

**4. CSV Data Export (Ref: `requirements.md` FR4.1)**
    *   P&L results (especially from Batch Mode) must be exportable to CSV.
    *   The CSV structure (columns, data format, sorting) must replicate that produced by `src/modules/exporter.ts`.

**5. DexScreener Monitoring & Wallet Discovery (Ref: `requirements.md` FR2)**
    *   `dex_client` uses DexScreener WebSocket for trending pair streams and HTTP API for specific pair data.
    *   It processes this data to identify and extract Solana wallet addresses (replicating logic from `extractSolKeys.js`, `workerTrending.js`).
    *   Discovered wallets are published to a Redis list for the Continuous Mode P&L pipeline.
    *   The DexScreener `trending:{pair}` keys are used in Redis to manage processing state of pairs.

## API Interaction Summary

The Rust rewrite will expose its functionalities via a RESTful API using the Axum framework. All payloads are JSON. For full endpoint details, request/response schemas, and examples, Claude should refer to the finalized `api_descriptions.md`.

**Key Internal API Endpoint Groups (Rust Rewrite):**

*   **Configuration (`/api/config`):**
    *   `GET /api/config`: Retrieve current system configuration (reflecting JS `.env` params and new Rust configurables like DexScreener/Jupiter URLs).
    *   `POST /api/config`: Update system configuration. (Sensitive; requires authentication).
*   **Batch P&L Analysis (`/api/pnl/batch/*`):**
    *   `POST /api/pnl/batch/run`: Submit wallet lists (and optional P&L config overrides) for batch analysis. (Asynchronous; requires authentication).
    *   `GET /api/pnl/batch/status/{run_id}`: Check batch run status.
    *   `GET /api/pnl/batch/results/{run_id}`: Get completed batch run results.
    *   `GET /api/pnl/batch/results/{run_id}/export.csv`: Download CSV results (format matching `exporter.ts`).
*   **Continuous Mode Insights (`/api/pnl/continuous/*`):**
    *   `GET /api/pnl/continuous/discovered-wallets`: List wallets discovered via DexScreener and analyzed by the continuous P&L pipeline. Includes filtering/pagination.
    *   `GET /api/pnl/continuous/discovered-wallets/{wallet_address}/details`: Get detailed P&L for a specific discovered wallet.
*   **Dex Monitoring Service (`/api/dex/*`):**
    *   `GET /api/dex/status`: Provides status of the `dex_client` service (DexScreener monitoring).
    *   Optional `POST /api/dex/control` (`{ "action": "start"|"stop" }`): To manage the `dex_client` service.
*   **System Status (`/api/status`, `/api/logs`):** For health checks and log retrieval.

**External APIs Consumed (by Rust Rewrite):**

*   **Primary: Solana RPC API:** For fetching transaction signatures and details. (Configurable URL).
*   **DexScreener APIs:**
    *   WebSocket (`wss://io.dexscreener.com/dex/screener/v5/pairs/h24/1`): For live trending pair data by `dex_client`. (URL to be configurable).
    *   HTTP API (`https://io.dexscreener.com/dex/log/amm/v4/pumpfundex/top/solana/{pair}`): For specific pair details by `dex_client`. (Base URL to be configurable).
*   **Jupiter Price API (`https://lite-api.jup.ag/price/v2`):** For token prices by `jprice_client`. (URL to be configurable).
*   The system must robustly handle rate limits and errors from these external services.

## Development Roadmap / Phasing

Development will follow a phased approach, building foundational components first. Claude should refer to `project_planning.md` for detailed tasks within each phase.

*   **Phase 1: Core Components & Workspace Setup**
    *   Focus: Rust workspace, all crates (incl. `pnl_core`, `solana_client`, `dex_client`, `jprice_client`, `tx_parser`, `persistence_layer`, `api_server`, `config_manager`, `job_orchestrator`).
    *   Implement core P&L logic (replicating JS), Solana RPC interaction, DexScreener data fetching basics, Jupiter price fetching, transaction parsing, initial Redis setup, and config management. Unit tests for core logic.

*   **Phase 2: API Server & Batch Mode Functionality**
    *   Focus: Basic Axum `api_server`, config endpoints.
    *   Develop Batch Analysis Mode: `job_orchestrator` for batch jobs, `/api/pnl/batch/*` endpoints, P&L pipeline integration, CSV export. Batch mode tests.

*   **Phase 3: Dex Client & Continuous Mode Functionality**
    *   Focus: Full `dex_client` implementation (DexScreener wallet discovery & Redis push).
    *   Enhance `persistence_layer` for continuous mode Redis features (`aggregator-lock`, wallet queue, `trending:{pair}` state, price cache).
    *   Integrate `dex_client` discoveries with `job_orchestrator` (Redis queue consumption, `aggregator-lock` usage).
    *   Implement `/api/pnl/continuous/*` and `/api/dex/*` endpoints.
    *   Setup continuous background service for Dex monitoring. Continuous mode tests.

*   **Phase 4: Cross-Cutting Concerns & Finalization**
    *   Focus: Comprehensive logging (`tracing`), error handling, API security.
    *   Performance profiling/optimization. Documentation (`rustdoc`, basic user guide). Build/deployment setup (Docker). Final review.

## Common Commands
```bash
cargo build
```
```bash
cargo test
```
```bash
cargo test -p <crate_name>
# Example: cargo test -p pnl_core
```
```bash
cargo run -p api_server
```
```bash
cargo check
```
```bash
cargo fmt
```
```bash
cargo clippy
```

## Development Notes & Best Practices

-   **Project Stage:** This is a rewrite of an existing JS/TS system. The initial task is to replicate the "as-is" functionality in Rust. Crates are at v0.1.0, and much detailed logic needs to be implemented by translating from the JS/TS codebase (`Full_sys_tim/`). Start by creating the basic structure and functions within each crate as per Phase 1 of the Roadmap.
-   **Workspace Structure:** This is a Cargo workspace. Ensure code is organized within the appropriate crates (e.g., `pnl_core`, `api_server`, `dex_client`, `jprice_client`).
-   **Technology Stack (Rust Rewrite):**
    *   **Core:** Rust (latest stable edition).
    *   **API:** Axum framework on Tokio.
    *   **Async Runtime:** Tokio for all asynchronous operations.
    *   **Data Persistence/Caching/Queueing:** Redis, using the `redis-rs` crate with Tokio integration.
    *   **External Data Sources:** Solana RPC, DexScreener (WebSocket & HTTP), Jupiter API (for prices).
-   **Coding Standards & Detailed Guidelines:**
    *   **Formatting:** Strictly use `cargo fmt`.
    *   **Linting:** Regularly run `cargo clippy --all-targets --all-features -- -D warnings` and address all issues.
    *   **Primary Guidelines Document:** For comprehensive Rust coding standards, including detailed error handling strategy (using `thiserror` for library errors, `anyhow` for application logic), logging strategy (using the `tracing` crate), testing methodologies, dependency management, and specific advice for interacting with project components (like `persistence_layer` Redis patterns), **refer extensively to the `ai_coding_guidelines.md` file in this repository.** Adherence to `ai_coding_guidelines.md` is critical for consistency and quality.
-   **Modularity & Replication:**
    *   Maintain clear separation of concerns between crates as defined in `architecture.md`.
    *   The initial goal for many modules (e.g., `pnl_core`, `tx_parser`, wallet extraction logic in `dex_client`) is to faithfully replicate the logic from their JS/TS counterparts (e.g., `pnl.ts`, `txParser.ts`, `extractSolKeys.js`). Claude should be prepared to analyze JS/TS code and translate its logic to idiomatic Rust.
-   **Configuration:**
    *   All configurable parameters (mirroring JS `.env` variables, plus new ones like DexScreener/Jupiter URLs) must be managed via `config_manager` and accessible through a shared, type-safe `SystemConfig` struct.
-   **Error Handling:**
    *   Expect and robustly handle errors from all external API calls (Solana RPC, DexScreener, Jupiter, Redis) and I/O operations. Implement intelligent retry mechanisms with backoff where appropriate (especially in `solana_client`, `dex_client`, `jprice_client`).
-   **Testing:**
    *   Unit tests are vital for core logic (P&L calculations, transaction parsing).
    *   Integration tests are crucial for verifying inter-crate data flows (e.g., `dex_client` -> Redis -> `job_orchestrator` -> P&L pipeline).
-   **Asynchronous Operations:**
    *   The system is heavily asynchronous. Use `async/await` for all I/O. Be mindful of `Send` and `Sync` requirements when sharing data across Tokio tasks.
-   **Performance Goals:** Remember that significant performance and parallelism improvements over the JS version are key objectives for this Rust rewrite. Design choices should reflect this.

## Key Challenges / Focus Areas

During development, particular attention should be paid to the following areas:

*   **P&L Calculation Accuracy:** Precisely replicating the P&L calculation logic from the existing JS/TS codebase (`pnl.ts`, `txParser.ts`), including how it handles transfers, timeframes, fees, and uses Jupiter prices (via `jprice_client`), is paramount. Rigorous testing against outputs from the JS system with the same inputs will be necessary.
*   **Performance & Parallelism:**
    *   Achieving significant speedup in fetching (Solana transactions, DexScreener data, Jupiter prices) and processing (P&L calculations).
    *   Effectively utilizing Tokio for concurrency: parallel wallet processing in Batch Mode, efficient async operations within the single-wallet processing sequence of Continuous Mode (post `aggregator-lock`).
    *   Optimizing Redis interactions.
*   **Robustness of External Interactions:**
    *   Graceful error handling (timeouts, rate limits, unexpected responses) for Solana RPC, DexScreener (WebSocket & HTTP), and Jupiter API. Implement resilient retry strategies.
    *   Ensuring the `dex_client`'s continuous DexScreener monitoring is stable and can recover from temporary disruptions.
*   **State Management & Replication in Continuous Mode:**
    *   Correctly implementing the Redis-based wallet queue (`dex_client` producing, `job_orchestrator` consuming).
    *   Faithfully replicating the `aggregator-lock` logic from `src/Main.ts` for the P&L processing loop.
    *   Accurately managing other Redis state from the JS system (e.g., `trending:{pair}` processing status, `accamounts:*`, `temptxids:*`).
*   **Configuration Management:** Ensuring `config_manager` correctly loads all necessary parameters (from JS `.env` equivalents plus new Rust configurables) and provides them type-safely.
*   **JS/TS Logic Translation:** Accurately translating nuanced JavaScript logic (especially in P&L calculations, transaction parsing, and DexScreener data processing for wallet extraction) into idiomatic and performant Rust.
