use super::*;

#[cfg(feature = "electrum")]
#[test]
#[parallel]
fn success() {
    initialize();

    let amount = 1000;
    let expiration = 3600;
    let (mut wallet, online) = get_funded_wallet!();

    // Test 1: Create a simple P2WSH script (hash lock)
    use crate::bitcoin::script::Builder;
    use crate::bitcoin::opcodes::all::*;
    use crate::bitcoin::hashes::{sha256, Hash as HashTrait};

    let payment_hash = <sha256::Hash as HashTrait>::hash(b"test_preimage").to_byte_array();
    let custom_script = Builder::new()
        .push_opcode(OP_SHA256)
        .push_slice(&payment_hash)
        .push_opcode(OP_EQUAL)
        .into_script();

    // Test default expiration + min confirmations
    let bak_info_before = wallet.database.get_backup_info().unwrap().unwrap();
    let now_timestamp = now().unix_timestamp();
    
    let receive_data = wallet
        .script_receive(
            custom_script.clone(),
            None,
            Assignment::Any,
            None,
            TRANSPORT_ENDPOINTS.clone(),
            MIN_CONFIRMATIONS,
        )
        .unwrap();

    let bak_info_after = wallet.database.get_backup_info().unwrap().unwrap();
    assert!(bak_info_after.last_operation_timestamp > bak_info_before.last_operation_timestamp);
    
    // Verify default expiration
    assert!(receive_data.expiration_timestamp.is_some());
    let timestamp = now_timestamp + DURATION_RCV_TRANSFER as i64;
    assert!(receive_data.expiration_timestamp.unwrap() - timestamp <= 1);

    // Verify invoice
    let decoded_invoice = Invoice::new(receive_data.invoice.clone()).unwrap();
    assert_eq!(
        decoded_invoice.invoice_data.network,
        wallet.bitcoin_network()
    );

    // Verify transfer was created
    let transfer = get_test_transfer_recipient(&wallet, &receive_data.recipient_id);
    let (_, batch_transfer) = get_test_transfer_related(&wallet, &transfer);
    assert_eq!(batch_transfer.min_confirmations, MIN_CONFIRMATIONS);
    assert!(transfer.incoming);

    // Test 2: Custom expiration
    let now_timestamp = now().unix_timestamp();
    let receive_data = wallet
        .script_receive(
            custom_script.clone(),
            None,
            Assignment::Any,
            Some(expiration),
            TRANSPORT_ENDPOINTS.clone(),
            MIN_CONFIRMATIONS,
        )
        .unwrap();
    assert!(receive_data.expiration_timestamp.is_some());
    let timestamp = now_timestamp + expiration as i64;
    assert!(receive_data.expiration_timestamp.unwrap() - timestamp <= 1);

    // Test 3: Zero expiration (never expires)
    let receive_data = wallet
        .script_receive(
            custom_script.clone(),
            None,
            Assignment::Any,
            Some(0),
            TRANSPORT_ENDPOINTS.clone(),
            MIN_CONFIRMATIONS,
        )
        .unwrap();
    assert!(receive_data.expiration_timestamp.is_none());

    let min_confirmations = 2;
    let receive_data = wallet
        .script_receive(
            custom_script.clone(),
            None,
            Assignment::Any,
            None,
            TRANSPORT_ENDPOINTS.clone(),
            min_confirmations,
        )
        .unwrap();
    let transfer = get_test_transfer_recipient(&wallet, &receive_data.recipient_id);
    let (_, batch_transfer) = get_test_transfer_related(&wallet, &transfer);
    assert_eq!(batch_transfer.min_confirmations, min_confirmations);

    let asset = test_issue_asset_cfa(&mut wallet, &online, None, None);
    let asset_id = asset.asset_id;
    
    let result = wallet.script_receive(
        custom_script.clone(),
        Some(asset_id.clone()),
        Assignment::Any,
        None,
        TRANSPORT_ENDPOINTS.clone(),
        MIN_CONFIRMATIONS,
    );
    assert!(result.is_ok());

    let now_timestamp = now().unix_timestamp();
    let result = wallet.script_receive(
        custom_script.clone(),
        Some(asset_id.clone()),
        Assignment::Fungible(amount),
        Some(expiration),
        TRANSPORT_ENDPOINTS.clone(),
        MIN_CONFIRMATIONS,
    );
    assert!(result.is_ok());
    let receive_data = result.unwrap();

    let invoice = Invoice::new(receive_data.invoice).unwrap();
    let invoice_data = invoice.invoice_data();
    let approx_expiry = now_timestamp + expiration as i64;
    assert_eq!(invoice_data.recipient_id, receive_data.recipient_id);
    assert_eq!(invoice_data.asset_schema, Some(AssetSchema::Cfa));
    assert_eq!(invoice_data.asset_id, Some(asset_id));
    assert_eq!(invoice_data.assignment, Assignment::Fungible(amount));
    assert_eq!(invoice_data.network, BitcoinNetwork::Regtest);
    assert!(invoice_data.expiration_timestamp.unwrap() - approx_expiry <= 1);
    assert_eq!(
        invoice_data.transport_endpoints,
        TRANSPORT_ENDPOINTS.clone()
    );
}

#[cfg(feature = "electrum")]
#[test]
#[parallel]
fn success_htlc_script() {
    initialize();

    let (mut wallet, _online) = get_funded_wallet!();

    // Create a realistic HTLC script
    use crate::bitcoin::script::Builder;
    use crate::bitcoin::opcodes::all::*;
    use crate::bitcoin::hashes::{sha256, Hash as HashTrait};
    use crate::bitcoin::PublicKey;
    use std::str::FromStr;

    let payment_hash = <sha256::Hash as HashTrait>::hash(b"htlc_preimage").to_byte_array();
    let lp_pubkey = PublicKey::from_str(
        "02a1633cafcc01ebfb6d78e39f687a1f0995c62fc95f51ead10a02ee0be551b5dc"
    ).unwrap();
    let user_pubkey = PublicKey::from_str(
        "03d6c27614557184d269b9cb19b1bc32479e661d86a925f4c4e46c734adcea3d19"
    ).unwrap();
    let timelock_blocks = 144;

    let htlc_script = Builder::new()
        .push_opcode(OP_IF)
            .push_opcode(OP_SHA256)
            .push_slice(&payment_hash)
            .push_opcode(OP_EQUALVERIFY)
            .push_key(&lp_pubkey)
            .push_opcode(OP_CHECKSIG)
        .push_opcode(OP_ELSE)
            .push_int(timelock_blocks)
            .push_opcode(OP_CSV)
            .push_opcode(OP_DROP)
            .push_key(&user_pubkey)
            .push_opcode(OP_CHECKSIG)
        .push_opcode(OP_ENDIF)
        .into_script();

    let receive_data = wallet
        .script_receive(
            htlc_script,
            None,
            Assignment::Any,
            Some(86400), // 24 hours
            TRANSPORT_ENDPOINTS.clone(),
            1,
        )
        .unwrap();

    assert!(!receive_data.invoice.is_empty());
    assert!(!receive_data.recipient_id.is_empty());
    assert!(receive_data.expiration_timestamp.is_some());
    
    // Verify transfer was created
    let transfer = get_test_transfer_recipient(&wallet, &receive_data.recipient_id);
    assert!(transfer.incoming);
}

#[test]
#[parallel]
fn fail_invalid_transport() {
    initialize();

    let (mut wallet, _online) = get_funded_noutxo_wallet!();
    
    use crate::bitcoin::script::Builder;
    use crate::bitcoin::opcodes::all::OP_PUSHNUM_1;
    let simple_script = Builder::new().push_opcode(OP_PUSHNUM_1).into_script();

    // Invalid transport endpoint
    let result = wallet.script_receive(
        simple_script,
        None,
        Assignment::Any,
        None,
        vec!["invalid://endpoint".to_string()],
        MIN_CONFIRMATIONS,
    );
    
    assert!(matches!(result, Err(Error::InvalidTransportEndpoint { .. })));
}

#[test]
#[parallel]
fn fail_invalid_asset() {
    initialize();

    let (mut wallet, _online) = get_funded_noutxo_wallet!();
    
    use crate::bitcoin::script::Builder;
    use crate::bitcoin::opcodes::all::OP_PUSHNUM_1;
    let simple_script = Builder::new().push_opcode(OP_PUSHNUM_1).into_script();

    // Non-existent asset
    let result = wallet.script_receive(
        simple_script,
        Some("rgb1invalid".to_string()),
        Assignment::Any,
        None,
        TRANSPORT_ENDPOINTS.clone(),
        MIN_CONFIRMATIONS,
    );
    
    assert!(matches!(result, Err(Error::AssetNotFound { .. })));
}

