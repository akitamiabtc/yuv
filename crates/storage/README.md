# `yuv-storage`

Provides traits and implementations of storage for YUV transactions. For default
use case it is a wrapper around `LevelDB` database, for tests - in-memory storage.

All the types that come through the storage are serialized using `ciborium`.

Example of using the [InventoryStorage](src/traits/inventory.rs):

```rust
use std::str::FromStr;
use yuv_storage::{InventoryStorage, LevelDB};
use bitcoin::Txid;

tokio_test::block_on(async {
    // Init the DB.
    let db = LevelDB::in_memory().expect("LevelDB should init");

    // Put a vector of Txids to the DB.
    db.put_inventory(vec![Txid::from_str(
        "b4f45a2e3857b1b5f74ca7ed81a95b039baa89a49b1fd41b96e47afb129c0810",
    ).unwrap()])
        .await
        .expect("Inventory should be put");

    // Get the inventory from the DB.
    let inventory = db
        .get_inventory()
        .await
        .expect("Inventory should be retrieved");

    println!("Inventory: {:?}", inventory);
});
```

> The code is pretty much the same for all the other [traits](src/traits/).
