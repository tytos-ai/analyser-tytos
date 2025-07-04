
import json

def analyze_transaction_structure(filename):
    """Analyzes the structure of a transaction JSON file."""
    with open(filename, 'r') as f:
        data = json.load(f)

    print(f"Analyzing file: {filename}")
    print("Top-level keys:", list(data.keys()))

    if "data" in data and "items" in data["data"] and data["data"]["items"]:
        first_transaction = data["data"]["items"][0]
        print("\nFirst transaction keys and data types:")
        for key, value in first_transaction.items():
            print(f"  - {key}: {type(value).__name__}")

        print(f"\nTotal transactions in file: {len(data['data']['items'])}")
    else:
        print("\nNo transactions found in the file.")

if __name__ == "__main__":
    # Use one of the downloaded transaction files
    filename = "transactions_8Bu2Lmdu5KYKfJJ9nuAjnT5CUhDSCweyUwuTfXQrmDqs.json"
    analyze_transaction_structure(filename)

