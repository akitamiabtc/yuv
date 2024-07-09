use std::collections::VecDeque;

use crate::handler::update_inv;

const MAX_INV_SIZE: usize = 10;

type Txid = u64;

fn do_test_update_inv_with_max(
    inv: Vec<Txid>,
    txs: Vec<Txid>,
    expected: Vec<Txid>,
    max_inv_size: usize,
) -> VecDeque<Txid> {
    let mut invq = VecDeque::with_capacity(max_inv_size);
    invq.extend(inv);

    update_inv(&mut invq, &txs, max_inv_size);

    assert_eq!(invq, expected.into_iter().collect::<VecDeque<Txid>>());

    invq
}

fn do_test_update_inv(inv: Vec<Txid>, txs: Vec<Txid>, expected: Vec<Txid>) {
    let invq = do_test_update_inv_with_max(inv, txs, expected, MAX_INV_SIZE);
    assert_eq!(
        invq.capacity(),
        MAX_INV_SIZE,
        "capacity of inventory shouldn't change"
    );
}

fn do_test_update_inv_with_changed_max(
    inv: Vec<Txid>,
    txs: Vec<Txid>,
    expected: Vec<Txid>,
    new_max: usize,
) {
    do_test_update_inv_with_max(inv, txs, expected, new_max);
}

#[test]
fn test_update_inv() {
    // extend existing inv
    let inv = vec![1, 2, 3, 4, 5];
    let new_txs = vec![6, 7, 8, 9, 10];
    let expected = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    do_test_update_inv(inv, new_txs, expected);

    let inv = vec![1, 2, 3, 4, 5];
    let new_txs = vec![6, 7, 8, 9, 10, 11];
    let expected = vec![2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
    do_test_update_inv(inv, new_txs, expected);

    let inv = vec![1, 2, 3, 4, 5];
    let new_txs = vec![6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
    let expected = vec![6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
    do_test_update_inv(inv, new_txs, expected);

    let inv = vec![1, 2, 3, 4, 5];
    let new_txs = vec![6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20];
    let expected = vec![11, 12, 13, 14, 15, 16, 17, 18, 19, 20];
    do_test_update_inv(inv, new_txs, expected);
}

#[test]
fn test_max_inv_size_changed() {
    // Max size is reduced
    let inv = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let new_txs = vec![11, 12, 13, 14, 15];
    let max_size = 5;
    let expected = vec![11, 12, 13, 14, 15];
    do_test_update_inv_with_changed_max(inv, new_txs, expected, max_size);

    // Max size is increased
    let inv = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let new_txs = vec![11, 12, 13, 14, 15];
    let max_size = 15;
    let expected = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
    do_test_update_inv_with_changed_max(inv, new_txs, expected, max_size);

    // Max size reduced and new txs are more than max size
    let inv = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let new_txs = vec![11, 12, 13, 14, 15, 16, 17, 18, 19, 20];
    let max_size = 5;
    let expected = vec![16, 17, 18, 19, 20];
    do_test_update_inv_with_changed_max(inv, new_txs, expected, max_size);

    // Max size increased and new txs are more than max size
    let inv = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let new_txs = vec![11, 12, 13, 14, 15, 16, 17, 18, 19, 20];
    let max_size = 15;
    let expected = vec![6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20];
    do_test_update_inv_with_changed_max(inv, new_txs, expected, max_size);
}
