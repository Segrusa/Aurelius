# The vault (server): its key, what runs get, and what it does not protect

The vault keeps the company's and the projects' texts, URLs and secrets
(multi-user phase 4). Every value is encrypted in the database with the
server's vault key; a work run gets the items of the company and of its
project as environment variables, and every secret value is replaced by
`«secret:NAME»` wherever the server stores or forwards text. This page is for
whoever runs the server: where the key is, how to move it to another machine,
which names a run may get, and the risks that remain.

## The key

- **macOS and Windows** keep the key in the system's credential store
  (Keychain, Credential Manager) for the server's own data folder.
- **Linux**, and every test instance (another data folder), keep it in
  `<data folder>/secrets/`, a folder of mode 0700 with a file of mode 0600. A
  key file others may read is not used (and never replaced): make it private
  (`chmod 600`) and the server reads it again, without a restart.
- **Windows with `AURELIUS_SECRETS=file`** stores the key as a file too, but
  does **not** check or set the file's permissions (no ACL). It is meant for
  test instances only; never use it for a real server.
- The key is made with the first vault item and never again while values are
  stored. Without it the vault is "undecryptable": nothing opens, runs get
  nothing from it, the rest of Aurelius works.

### Moving the server: export and import

1. **Export** (a board member, in the app): *Vault → Export key* asks for your
   password again. Every board member sees a line in the Team chat that you
   exported it; without that line (no general Team chat) the key is not
   exported. Whoever has the key opens every vault value and every backup of
   this server: keep it like a password.
2. **Import** on the new machine, with the Aurelius daemon stopped, from a
   file or a pipe, never typed in (the terminal would show it) and never as
   an argument (`ps`, the shell's history):

   ```sh
   aurelius-daemon vault-key import < key.txt
   # or, with a file only you may read (chmod 600):
   aurelius-daemon vault-key import --file key.txt
   ```

   The import refuses a key that does not open the stored values (another
   server's) and never replaces a key that encrypts values. Start Aurelius
   again afterwards. Delete `key.txt` once the server runs.

## What a run gets

- Only a **work run** (Claude Code with its tools: chats, tasks, a ticket's
  main task and its delegations). Text-only runs, triage and summaries get
  nothing.
- The items of the company and of the run's project; a project item with the
  name or key of a company item replaces it there. A project's items never
  reach another project's runs.
- Each run is audited per item (`deliver`, with the run's employee, task and
  ticket); `vault_list`/`vault_read` are audited as `tool_read`.
- **Names a run may not have.** The names the runtime needs or sets itself
  (`PATH`, `HOME`, `LANG`, `TMPDIR`, …), and everything that steers the claude
  process, its shells or its traffic: `ANTHROPIC_*`, `CLAUDE_*` (also
  `CLAUDE_CODE_USE_BEDROCK`, `_VERTEX`, `_FOUNDRY`), `VERTEX_*`, `BEDROCK_*`,
  `AURELIUS_*`, `NODE_*`, `LD_*`, `DYLD_*`, `LC_*`, `XDG_*`, `OTEL_*`, `MCP_*`,
  `SSL_CERT_*`, `*_PROXY`, the shell hooks (`BASH_ENV`, `ENV`, `PROMPT_COMMAND`,
  …) and the CA bundles. The cloud's own credentials (`AWS_*`, `GOOGLE_*`,
  `AZURE_*`) are allowed: Claude Code only uses them behind its provider
  switches, which stay refused, so they go to the tools the run calls. An
  item whose key a newer Aurelius refuses is not delivered, and the Team chat
  says so once per item.
- Texts and URLs go to runs as well, but they are **not** redacted: anything
  that must stay hidden is a *secret*.

## Redaction

Every secret value (current, previous versions, deleted items) is replaced
by `«secret:NAME»` in a run's events (the answer, tool input and output,
traces), in everything a run sends through Aurelius' tools (messages, the
discussion, tasks, ideas, memory, approvals, push), in the daemon's log
(panics too). A board member who pastes a secret's value in
a chat, a discussion, a ticket, a task or a project note is warned with the
item's name; a project note with one is refused.

The forms it recognises: the value as it is; base64 (standard and url-safe, at
each of the three byte offsets, with or without padding); hex in small and
capital letters; percent-encoding in both cases, also with `/` left as it is
(Python's `quote`) and form-encoding; only the spaces encoded (`%20` or `+`);
JSON escaping once and twice, with `\uXXXX` in small and capital letters and
.NET's escaping of `"&'+<>`.

## Residual risks

These are accepted (Sentinel SEC0/SEC1); know them before you store a value.

- **R1, Claude Code's own transcript.** Claude Code writes its session
  transcript in the board member's Claude login folder. What the run printed
  there (a value included) is Claude Code's, not Aurelius': Aurelius cannot
  redact it.
- **R2, MCP servers inherit the environment.** An MCP server the run starts
  gets the run's environment, vault variables included. Add only MCP servers
  you trust.
- **R3, the key on the same disk.** On Linux (and Windows test instances) the
  key file is on the same disk as the database; a run's Bash runs as the same
  user and could read both (Aurelius' file tools are denied `secrets/`, and
  the Bash commands that print the environment are refused in runs with vault
  values, but a run's own script is not stopped). Whoever can read the server's disk as this user can
  open the vault.
- **R5, values a run rewrites.** A value split up, reversed, encrypted, encoded
  in a form not listed above (shell escaping `\ `, partial encodings, other
  escapes) or written in pieces is not recognised. Base64 keeps up to one
  character at each end of the encoded value (at most a few bits of its first
  and last byte); base64 wrapped over lines is not recognised.
- **Files a run writes are not redacted.** What an agent writes into a
  project's files and the artifacts it publishes (files on disk, shareable)
  stays as it is; only their title and URL are redacted. Instructions tell
  runs never to write a value anywhere, but that is not enforced.
