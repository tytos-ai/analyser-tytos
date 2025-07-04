
import requests
import json
import time

API_KEY = "5ff313b239ac42e297b830b10ea1871d"
BASE_URL = "https://public-api.birdeye.so/trader/txs/seek_by_time"

headers = {
    "X-API-KEY": API_KEY,
    "accept": "application/json",
    "x-chain": "solana"
}

def get_transactions(address, offset=0, limit=100):
    """Fetches transactions for a given address."""
    params = {
        "address": address,
        "offset": offset,
        "limit": limit
    }
    response = requests.get(BASE_URL, headers=headers, params=params)
    response.raise_for_status()  # Raise an exception for bad status codes
    return response.json()

def save_transactions(address, transactions, filename):
    """Saves transactions to a JSON file."""
    with open(filename, 'w') as f:
        json.dump(transactions, f, indent=4)

def fetch_and_save_all_transactions(address, total_transactions=400, limit=100):
    """Fetches all transactions for an address and saves them to a file."""
    all_transactions = {"data": {"items": []}}
    for offset in range(0, total_transactions, limit):
        try:
            print(f"Fetching transactions for {address} with offset {offset}...")
            data = get_transactions(address, offset, limit)
            if data.get("success") and data.get("data", {}).get("items"):
                all_transactions["data"]["items"].extend(data["data"]["items"])
            else:
                print(f"No more transactions found for {address} at offset {offset}.")
                break
            time.sleep(1)  # Respect rate limits
        except requests.exceptions.RequestException as e:
            print(f"An error occurred: {e}")
            break

    filename = f"transactions_{address}.json"
    save_transactions(address, all_transactions, filename)
    print(f"Saved {len(all_transactions['data']['items'])} transactions to {filename}")

if __name__ == "__main__":
    wallet_addresses = [
        "8Bu2Lmdu5KYKfJJ9nuAjnT5CUhDSCweyUwuTfXQrmDqs",
        "YubQzu18FDqJRyNfG8JqHmsdbxhnoQqcKUHBdUkN6tP",
        "7dGrdJRYtsNR8UYxZ3TnifXGjGc9eRYLq9sELwYpuuUu",
        "H3SsPcVhZWhRyupU87Hn95WrJaYdz4rWNpdaiS4BSwmm",
        "4GQeEya6ZTwvXre4Br6ZfDyfe2WQMkcDz2QbkJZazVqS",
    ]
    for address in wallet_addresses:
        fetch_and_save_all_transactions(address)
