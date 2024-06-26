use once_cell::sync::Lazy;
use yuv_types::YuvTransaction;

use crate::check_transaction;

static VALID_MULTICHROMA_TRANSFER: Lazy<YuvTransaction> = Lazy::new(|| {
    serde_json::from_str::<YuvTransaction>(include_str!("./assets/multichroma_valid_transfer.json"))
        .expect("JSON was not well-formatted")
});

static VALID_SINGLECHROMA_TRANSFER: Lazy<YuvTransaction> = Lazy::new(|| {
    serde_json::from_str::<YuvTransaction>(include_str!(
        "./assets/singlechroma_valid_transfer.json"
    ))
    .expect("JSON was not well-formatted")
});

static INVALID_MULTICHROMA_TRANSFER: Lazy<YuvTransaction> = Lazy::new(|| {
    serde_json::from_str::<YuvTransaction>(include_str!(
        "./assets/multichroma_invalid_transfer.json"
    ))
    .expect("JSON was not well-formatted")
});

static INVALID_SINGLECHROMA_TRANSFER: Lazy<YuvTransaction> = Lazy::new(|| {
    serde_json::from_str::<YuvTransaction>(include_str!(
        "./assets/singlechroma_invalid_transfer.json"
    ))
    .expect("JSON was not well-formatted")
});

#[tokio::test]
async fn test_tx_checker_validates_multichroma_transfer() {
    let result = check_transaction(&VALID_MULTICHROMA_TRANSFER);

    assert!(result.is_ok(), "expected the tx to pass the check");
}

#[tokio::test]
async fn test_tx_checker_validates_singlechroma_transfer() {
    let result = check_transaction(&VALID_SINGLECHROMA_TRANSFER);

    assert!(result.is_ok(), "expected the tx to pass the check");
}

#[tokio::test]
async fn test_tx_checker_fails_invalid_multichroma_transfer() {
    let result = check_transaction(&INVALID_MULTICHROMA_TRANSFER);

    assert!(result.is_err(), "expected the tx to fail the check");
}

#[tokio::test]
async fn test_tx_checker_fails_invalid_singlechroma_transfer() {
    let result = check_transaction(&INVALID_SINGLECHROMA_TRANSFER);

    assert!(result.is_err(), "expected the tx to fail the check");
}
