# Project Description: Rust P&L Tracker and Dex Analyzer (As-Is Rewrite Focus)

## 1. Overview

This project undertakes the rewrite of an existing Profit & Loss (P&L) calculation and Decentralized Exchange (Dex) analysis system from Node.js/TypeScript to Rust. The primary objectives for this rewrite are to achieve **significant performance improvements**, enable **robust parallelism** in data fetching and processing, and expose all system functionalities via a **modern, comprehensive HTTP API** built with the Axum framework.

The system, once rewritten in Rust, will replicate the core functionalities of the current application, which include:
*   **Batch Analysis Mode:** Allowing users to submit specific lists of Solana wallet addresses for on-demand P&L calculation.
*   **Continuous Analysis Mode:** Proactively monitoring DexScreener data to discover potentially interesting trader wallets, which are then queued (via Redis) for P&L analysis. This mode is aimed at supporting copy trading research.

The focus is on a faithful translation of existing features and logic into a more performant and robust Rust implementation.

## 2. Core Functionality (to be replicated in Rust)

*   **Solana Wallet P&L Calculation:**
    *   Ingests Solana wallet addresses (from user-provided lists in Batch Mode, or from Redis queue in Continuous Mode).
    *   Fetches and processes historical transaction data from the Solana blockchain (via direct RPC).
    *   Calculates P&L based on configurable parameters (derived from current `.env` settings like timeframe, min capital) and specific P&L rules (derived from existing `pnl.ts`, `txParser.ts` logic). Token prices for calculations are sourced from the Jupiter API and cached in Redis.
    *   Outputs P&L results to CSV (matching `exporter.ts` format) and makes them available via API.
*   **DexScreener Monitoring & Wallet Discovery (for Continuous Analysis Mode):**
    *   Connects to DexScreener (WebSocket for trending pairs, HTTP API for specific pair data).
    *   Extracts wallet addresses from DexScreener data (replicating logic from `Dex/*` scripts like `extractSolKeys.js`).
    *   Pushes discovered wallets to a Redis queue for the P&L analysis pipeline.
*   **API-Driven Interaction (New Layer in Rust):**
    *   Exposes RESTful API endpoints for all system capabilities: initiating batch P&L runs, querying P&L results (batch and continuous), managing system configuration (mirroring current `.env` options), and monitoring system/Dex service status.
*   **Configuration Management:**
    *   Allows configuration of Solana RPC URL, Redis URL, P&L filter parameters (from `.env`), and making previously hardcoded URLs (DexScreener, Jupiter) configurable.

## 3. Key Goals for the Rust Rewrite

*   **Performance:** Leverage Rust's efficiency and Tokio's asynchronous capabilities for substantially faster data fetching (Solana transactions, DexScreener data, Jupiter prices) and P&L calculations compared to the Node.js version.
*   **Parallelism:** Implement concurrent operations for:
    *   Fetching data for multiple wallets in parallel (Batch Mode).
    *   Processing P&L for multiple wallets in parallel (Batch Mode).
    *   Efficient asynchronous handling of all I/O with external services (Solana RPC, DexScreener, Jupiter, Redis).
*   **Robust API Layer:** Provide a comprehensive and reliable Axum-based API for all system interactions.
*   **Reliability:** Enhance system stability and error handling, especially for long-running continuous mode operations.
*   **Maintainability:** Create a well-structured, modular Rust codebase for easier future development.

## 4. Target Users & Use Cases (Post-Rewrite)

*   Users of the existing system requiring better performance and API access.
*   Traders & Analysts needing efficient P&L tracking and tools for copy trading research based on Dex activity.
*   Developers seeking to integrate P&L or Dex-discovered wallet data via the new API.

## 5. Technology Stack (Rust Rewrite)

*   **Core Language:** Rust
*   **Web Framework:** Axum
*   **Async Runtime:** Tokio
*   **Primary Data Sources (External):** Solana RPC, DexScreener (WebSocket & HTTP), Jupiter API (for prices).
*   **Internal Data Handling/Queuing:** Redis.
*   **Configuration:** `config` crate with `.toml` files / environment variables.

This project aims to deliver a significantly more performant, robust, and versatile version of the existing P&L analysis tool, built on a modern Rust foundation.
