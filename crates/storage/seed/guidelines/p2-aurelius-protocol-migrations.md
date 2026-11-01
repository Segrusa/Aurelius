Protocol
- Stay inside the current major (5): add, never change or remove. A new capability gets a feature in `FEATURES` (`rpc.rs`); clients hide what the server lacks.
- New fields are optional with a default; old clients skip unknown events.
- Every new method gets its rights in `app.rs` (apps: `NOT_FOR_APPS` on writes by default) and `remote.rs`; a board-member write calls `require_board_member`.
- A method whose params may carry a secret or password is handled before the command pipeline (no value in `request_hash`, events or receipts) and is listed in `quotes_no_params`.
- After a protocol change, run `cargo xtask gen-dart` and commit the generated files on their own ("gen-dart after …"). Never edit `clients/mobile/lib/data/protocol/*` by hand.

Migrations
- Take the next free number across all open branches. Never edit a migration that is on main.
- Tables are STRICT, with a CHECK for every enum and length. History and audit tables are append-only (`no_update`/`no_delete` triggers).
- To change a CHECK or NOT NULL, rebuild the table in the same migration (`x_new`, INSERT … SELECT, DROP, RENAME, indexes again).
- Each migration gets a `schemaNN_tests.rs` that opens a database at the previous version with data (`open_until`) and checks the upgrade.
- A migration that drops data needs the Tech Lead and the Owner first.
