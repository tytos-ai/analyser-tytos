#!/bin/bash

################################################################################
# Zerion API Bug Reproduction Script
#
# This script replicates the Rust application's pagination behavior to help
# Zerion reproduce 500 errors that occur on high page numbers (e.g., page 300+)
#
# Usage: ./zerion_bug_reproduction.sh
################################################################################

set -e  # Exit on error (but we'll handle API errors specially)

# Configuration (matching Rust application)
WALLET_ADDRESS="HytEnZY8kd4cZqeVfintmBnZ2VfqQdfinbzZ2ot6mRNZ"
BASE_URL="https://api.zerion.io"
API_KEY="zk_prod_b0bbb7857c74422582eb39d50f970006"
AUTH_HEADER="Basic emtfcHJvZF9iMGJiYjc4NTdjNzQ0MjI1ODJlYjM5ZDUwZjk3MDAwNjo="
CURRENCY="usd"
PAGE_SIZE="100"
CHAIN_IDS="solana"
TRASH_FILTER="only_non_trash"
OPERATION_TYPES="trade,send,receive"
TIMEOUT="120"

# Output files
OUTPUT_DIR="./zerion_bug_logs_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$OUTPUT_DIR"
SUMMARY_LOG="$OUTPUT_DIR/summary.log"
ERROR_LOG="$OUTPUT_DIR/error.log"

# Color codes for terminal output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Initialize counters
PAGE_NUM=1
TOTAL_TRANSACTIONS=0
START_TIME=$(date +%s)

# Function to log messages
log() {
    echo -e "${BLUE}[$(date +'%Y-%m-%d %H:%M:%S')]${NC} $1" | tee -a "$SUMMARY_LOG"
}

log_success() {
    echo -e "${GREEN}[$(date +'%Y-%m-%d %H:%M:%S')]${NC} $1" | tee -a "$SUMMARY_LOG"
}

log_error() {
    echo -e "${RED}[$(date +'%Y-%m-%d %H:%M:%S')]${NC} $1" | tee -a "$SUMMARY_LOG" "$ERROR_LOG"
}

log_warn() {
    echo -e "${YELLOW}[$(date +'%Y-%m-%d %H:%M:%S')]${NC} $1" | tee -a "$SUMMARY_LOG"
}

# Initial URL
NEXT_URL="${BASE_URL}/v1/wallets/${WALLET_ADDRESS}/transactions/?currency=${CURRENCY}&page[size]=${PAGE_SIZE}&filter[chain_ids]=${CHAIN_IDS}&filter[trash]=${TRASH_FILTER}&filter[operation_types]=${OPERATION_TYPES}"

log "========================================="
log "Zerion API Bug Reproduction Script"
log "========================================="
log "Wallet: $WALLET_ADDRESS"
log "Chain: $CHAIN_IDS"
log "Page Size: $PAGE_SIZE"
log "Operation Types: $OPERATION_TYPES"
log "Trash Filter: $TRASH_FILTER"
log "Output Directory: $OUTPUT_DIR"
log "========================================="
log ""
log "Starting unlimited pagination fetch..."
log ""

# Pagination loop (matches Rust behavior)
while [ -n "$NEXT_URL" ]; do
    PAGE_START=$(date +%s%3N)

    log "Page $PAGE_NUM: Fetching from Zerion API..."
    log "URL: $NEXT_URL"

    # Make the request (matching Rust reqwest headers)
    HTTP_CODE=$(curl -w "%{http_code}" -o "$OUTPUT_DIR/page_${PAGE_NUM}_response.json" \
        -X GET "$NEXT_URL" \
        -H "Authorization: $AUTH_HEADER" \
        -H "Accept: */*" \
        -H "Accept-Encoding: gzip, deflate, br" \
        -H "Connection: keep-alive" \
        --max-time "$TIMEOUT" \
        --compressed \
        -s)

    PAGE_END=$(date +%s%3N)
    PAGE_ELAPSED=$((PAGE_END - PAGE_START))

    # Check HTTP status
    if [ "$HTTP_CODE" -ne 200 ]; then
        log_error "========================================="
        log_error "ERROR DETECTED ON PAGE $PAGE_NUM"
        log_error "========================================="
        log_error "HTTP Status Code: $HTTP_CODE"
        log_error "Page Number: $PAGE_NUM"
        log_error "URL: $NEXT_URL"
        log_error "Response Time: ${PAGE_ELAPSED}ms"
        log_error ""
        log_error "Response saved to: $OUTPUT_DIR/page_${PAGE_NUM}_response.json"
        log_error ""

        # Copy error response to separate file
        cp "$OUTPUT_DIR/page_${PAGE_NUM}_response.json" "$OUTPUT_DIR/ERROR_response.json"

        # Print response body
        log_error "Response Body:"
        cat "$OUTPUT_DIR/page_${PAGE_NUM}_response.json" | tee -a "$ERROR_LOG"
        log_error ""

        log_error "Stopping pagination due to API error."
        exit 1
    fi

    # Parse response
    RESPONSE_FILE="$OUTPUT_DIR/page_${PAGE_NUM}_response.json"

    # Count transactions in this page
    TX_COUNT=$(jq '.data | length' "$RESPONSE_FILE" 2>/dev/null || echo "0")
    TOTAL_TRANSACTIONS=$((TOTAL_TRANSACTIONS + TX_COUNT))

    # Get next URL from response
    NEXT_URL=$(jq -r '.links.next // empty' "$RESPONSE_FILE" 2>/dev/null)

    # Check if we got any transactions
    if [ "$TX_COUNT" -eq 0 ]; then
        log_warn "Page $PAGE_NUM: No more transactions, stopping pagination"
        NEXT_URL=""
        break
    fi

    # Log success
    HAS_NEXT="false"
    if [ -n "$NEXT_URL" ]; then
        HAS_NEXT="true"
    fi

    log_success "Page $PAGE_NUM: Fetched $TX_COUNT transactions in ${PAGE_ELAPSED}ms, has_next: $HAS_NEXT"

    # Check if there's a next page
    if [ -z "$NEXT_URL" ]; then
        log "No more pages available, pagination complete."
        break
    fi

    PAGE_NUM=$((PAGE_NUM + 1))

    # Optional: Add a small delay to mimic production behavior
    # sleep 0.2  # 200ms delay (matching config.toml rate_limit_delay_ms)
done

# Calculate final stats
END_TIME=$(date +%s)
TOTAL_ELAPSED=$((END_TIME - START_TIME))
AVG_PER_PAGE=$((TOTAL_ELAPSED / (PAGE_NUM > 0 ? PAGE_NUM : 1)))
TOTAL_PAGES=$((PAGE_NUM - 1))

log ""
log "========================================="
log "PAGINATION SUMMARY"
log "========================================="
log_success "Total Pages: $TOTAL_PAGES"
log_success "Total Transactions: $TOTAL_TRANSACTIONS"
log_success "Total Time: ${TOTAL_ELAPSED}s"
log "Average Time Per Page: ${AVG_PER_PAGE}s"
if [ "$TOTAL_ELAPSED" -gt 0 ]; then
    TX_PER_SEC=$((TOTAL_TRANSACTIONS / TOTAL_ELAPSED))
    log "Transactions Per Second: $TX_PER_SEC"
fi
log ""
log "All responses saved to: $OUTPUT_DIR"
log "Summary log: $SUMMARY_LOG"
log "========================================="
