use solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta;

fn main() {
    // Create a dummy transaction to see the structure
    let example: EncodedConfirmedTransactionWithStatusMeta = EncodedConfirmedTransactionWithStatusMeta {
        slot: 0,
        transaction: solana_transaction_status::EncodedTransactionWithStatusMeta {
            transaction: solana_transaction_status::EncodedTransaction::Json(
                solana_transaction_status::UiTransaction {
                    signatures: vec!["test".to_string()],
                    message: solana_transaction_status::UiMessage::Parsed(
                        solana_transaction_status::UiParsedMessage {
                            account_keys: vec![],
                            recent_blockhash: "test".to_string(),
                            instructions: vec![],
                            address_table_lookups: None,
                        }
                    ),
                }
            ),
            meta: Some(solana_transaction_status::UiTransactionStatusMeta {
                err: None,
                status: Ok(()),
                fee: 0,
                pre_balances: vec![],
                post_balances: vec![],
                inner_instructions: None,
                log_messages: None,
                pre_token_balances: None,
                post_token_balances: None,
                rewards: None,
                loaded_addresses: None,
                return_data: None,
                compute_units_consumed: None,
            }),
            version: None,
        },
        block_time: Some(0),
    };

    println!("Transaction has meta: {}", example.transaction.meta.is_some());
}