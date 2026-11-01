The crates point one way: domain → storage → application → daemon; protocol is the wire, runtime runs Claude Code.
- domain: pure types, validation and limits. No IO, no SQL.
- storage: all SQL lives here (rusqlite). The application and the daemon call storage functions; they never query tables themselves.
- application: `CommandService`. A write is a command that commits in one transaction with its events and receipt (idempotency); side effects follow the commit via `EventNotice`.
- daemon: the API (reads, writes, host, bridge), the dispatcher (runs) and permissions, as a thin layer over the application.
- protocol: wire types with `JsonSchema`; the mobile Dart models are generated from them.
- agent-bridge: the MCP tools a run sees. Each tool maps to an `EmployeeTaskOp` or `EmployeeMemoryOp` that the daemon checks; a tool call is never trusted.
- The system text of a run is built only in `application/src/run/plan.rs`; what changes often goes in its tail.
- What a board member types or a run produces passes the vault redactor before it is stored or sent. Never log params that can carry a secret.
