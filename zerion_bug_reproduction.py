#!/usr/bin/env python3

"""
Zerion API Bug Reproduction Script (Python Version)

This script replicates the Rust application's pagination behavior to help
Zerion reproduce 500 errors that occur on high page numbers (e.g., page 300+)

Usage: python3 zerion_bug_reproduction.py
"""

import requests
import json
import time
import base64
from datetime import datetime
from pathlib import Path
from typing import Optional, Dict, Any

# Color codes for terminal output
class Colors:
    RED = '\033[0;31m'
    GREEN = '\033[0;32m'
    YELLOW = '\033[1;33m'
    BLUE = '\033[0;34m'
    CYAN = '\033[0;36m'
    BOLD = '\033[1m'
    NC = '\033[0m'  # No Color

# Configuration (matching Rust application)
WALLET_ADDRESS = "HytEnZY8kd4cZqeVfintmBnZ2VfqQdfinbzZ2ot6mRNZ"
BASE_URL = "https://api.zerion.io"
API_KEY = "zk_prod_b0bbb7857c74422582eb39d50f970006"
CURRENCY = "usd"
PAGE_SIZE = 100
CHAIN_IDS = "solana"
TRASH_FILTER = "only_non_trash"
OPERATION_TYPES = "trade,send,receive"
TIMEOUT = 120  # seconds

# Generate Basic Auth header (matching Rust code)
def generate_auth_header() -> str:
    """Generate Basic Auth header by encoding API_KEY with trailing colon"""
    auth_string = f"{API_KEY}:"
    encoded = base64.b64encode(auth_string.encode()).decode()
    return f"Basic {encoded}"

# Setup output directory
OUTPUT_DIR = Path(f"./zerion_bug_logs_{datetime.now().strftime('%Y%m%d_%H%M%S')}")
OUTPUT_DIR.mkdir(parents=True, exist_ok=True)

SUMMARY_LOG = OUTPUT_DIR / "summary.log"
ERROR_LOG = OUTPUT_DIR / "error.log"

# Logging functions
def log(message: str, color: str = Colors.BLUE):
    """Log message to console and summary log"""
    timestamp = datetime.now().strftime('%Y-%m-%d %H:%M:%S')
    formatted = f"{color}[{timestamp}]{Colors.NC} {message}"
    print(formatted)
    with open(SUMMARY_LOG, 'a') as f:
        f.write(f"[{timestamp}] {message}\n")

def log_success(message: str):
    log(message, Colors.GREEN)

def log_error(message: str):
    timestamp = datetime.now().strftime('%Y-%m-%d %H:%M:%S')
    formatted = f"{Colors.RED}[{timestamp}]{Colors.NC} {message}"
    print(formatted)
    with open(SUMMARY_LOG, 'a') as f:
        f.write(f"[{timestamp}] {message}\n")
    with open(ERROR_LOG, 'a') as f:
        f.write(f"[{timestamp}] {message}\n")

def log_warn(message: str):
    log(message, Colors.YELLOW)

# Initialize HTTP session (matching Rust reqwest headers)
def create_session() -> requests.Session:
    """Create requests session with headers matching Rust reqwest client"""
    session = requests.Session()
    session.headers.update({
        'Authorization': generate_auth_header(),
        'Accept': '*/*',
        'Accept-Encoding': 'gzip, deflate, br',
        'Connection': 'keep-alive',
    })
    return session

def fetch_page(session: requests.Session, url: str, page_num: int) -> Dict[str, Any]:
    """
    Fetch a single page from Zerion API

    Returns:
        dict with keys: 'success', 'status_code', 'data', 'next_url', 'elapsed_ms', 'error'
    """
    start_time = time.time()

    try:
        log(f"Page {page_num}: Fetching from Zerion API...")
        log(f"URL: {url}")

        response = session.get(url, timeout=TIMEOUT)

        elapsed_ms = int((time.time() - start_time) * 1000)

        # Save response to file
        response_file = OUTPUT_DIR / f"page_{page_num}_response.json"
        with open(response_file, 'w') as f:
            try:
                json.dump(response.json(), f, indent=2)
            except:
                f.write(response.text)

        # Check status code
        if response.status_code != 200:
            error_data = {
                'success': False,
                'status_code': response.status_code,
                'data': None,
                'next_url': None,
                'elapsed_ms': elapsed_ms,
                'error': response.text,
                'response_file': str(response_file)
            }

            # Save error response separately
            error_file = OUTPUT_DIR / "ERROR_response.json"
            with open(error_file, 'w') as f:
                try:
                    json.dump(response.json(), f, indent=2)
                except:
                    f.write(response.text)

            return error_data

        # Parse JSON response
        data = response.json()
        transactions = data.get('data', [])
        next_url = data.get('links', {}).get('next')

        return {
            'success': True,
            'status_code': 200,
            'data': transactions,
            'next_url': next_url,
            'elapsed_ms': elapsed_ms,
            'error': None,
            'response_file': str(response_file)
        }

    except requests.exceptions.Timeout:
        elapsed_ms = int((time.time() - start_time) * 1000)
        return {
            'success': False,
            'status_code': 0,
            'data': None,
            'next_url': None,
            'elapsed_ms': elapsed_ms,
            'error': 'Request timeout'
        }
    except Exception as e:
        elapsed_ms = int((time.time() - start_time) * 1000)
        return {
            'success': False,
            'status_code': 0,
            'data': None,
            'next_url': None,
            'elapsed_ms': elapsed_ms,
            'error': str(e)
        }

def main():
    """Main pagination loop matching Rust application behavior"""

    # Print header
    log("=" * 60)
    log("Zerion API Bug Reproduction Script (Python)")
    log("=" * 60)
    log(f"Wallet: {WALLET_ADDRESS}")
    log(f"Chain: {CHAIN_IDS}")
    log(f"Page Size: {PAGE_SIZE}")
    log(f"Operation Types: {OPERATION_TYPES}")
    log(f"Trash Filter: {TRASH_FILTER}")
    log(f"Output Directory: {OUTPUT_DIR}")
    log("=" * 60)
    log("")
    log("Starting unlimited pagination fetch...")
    log("")

    # Create session
    session = create_session()

    # Initial URL
    next_url = (
        f"{BASE_URL}/v1/wallets/{WALLET_ADDRESS}/transactions/?"
        f"currency={CURRENCY}&"
        f"page[size]={PAGE_SIZE}&"
        f"filter[chain_ids]={CHAIN_IDS}&"
        f"filter[trash]={TRASH_FILTER}&"
        f"filter[operation_types]={OPERATION_TYPES}"
    )

    # Initialize counters
    page_num = 1
    total_transactions = 0
    start_time = time.time()

    # Pagination loop
    while next_url:
        result = fetch_page(session, next_url, page_num)

        # Check for errors
        if not result['success']:
            log_error("=" * 60)
            log_error(f"ERROR DETECTED ON PAGE {page_num}")
            log_error("=" * 60)
            log_error(f"HTTP Status Code: {result['status_code']}")
            log_error(f"Page Number: {page_num}")
            log_error(f"URL: {next_url}")
            log_error(f"Response Time: {result['elapsed_ms']}ms")
            log_error("")

            if result.get('response_file'):
                log_error(f"Response saved to: {result['response_file']}")

            log_error("")
            log_error("Error Details:")
            log_error(result['error'])
            log_error("")
            log_error("Stopping pagination due to API error.")

            # Print summary before exiting
            print_summary(page_num - 1, total_transactions, start_time)
            return 1

        # Count transactions
        tx_count = len(result['data'])
        total_transactions += tx_count

        # Check if we got any transactions
        if tx_count == 0:
            log_warn(f"Page {page_num}: No more transactions, stopping pagination")
            break

        # Log success
        has_next = result['next_url'] is not None
        log_success(
            f"Page {page_num}: Fetched {tx_count} transactions in "
            f"{result['elapsed_ms']}ms, has_next: {has_next}"
        )

        # Move to next page
        next_url = result['next_url']

        if not next_url:
            log("No more pages available, pagination complete.")
            break

        page_num += 1

        # Optional: Add rate limiting delay (matching config.toml)
        # time.sleep(0.2)  # 200ms delay

    # Print final summary
    print_summary(page_num, total_transactions, start_time)
    return 0

def print_summary(total_pages: int, total_transactions: int, start_time: float):
    """Print final pagination summary"""
    total_elapsed = int(time.time() - start_time)
    avg_per_page = total_elapsed // max(total_pages, 1)

    log("")
    log("=" * 60)
    log("PAGINATION SUMMARY")
    log("=" * 60)
    log_success(f"Total Pages: {total_pages}")
    log_success(f"Total Transactions: {total_transactions}")
    log_success(f"Total Time: {total_elapsed}s")
    log(f"Average Time Per Page: {avg_per_page}s")

    if total_elapsed > 0:
        tx_per_sec = total_transactions // total_elapsed
        log(f"Transactions Per Second: {tx_per_sec}")

    log("")
    log(f"All responses saved to: {OUTPUT_DIR}")
    log(f"Summary log: {SUMMARY_LOG}")
    log("=" * 60)

if __name__ == "__main__":
    try:
        exit_code = main()
        exit(exit_code)
    except KeyboardInterrupt:
        log_warn("\n\nScript interrupted by user (Ctrl+C)")
        log_warn("Partial results saved to: " + str(OUTPUT_DIR))
        exit(130)
    except Exception as e:
        log_error(f"\n\nUnexpected error: {e}")
        log_error("Partial results saved to: " + str(OUTPUT_DIR))
        import traceback
        traceback.print_exc()
        exit(1)
