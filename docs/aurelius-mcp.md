# aurelius-mcp: Aurelius for other apps and agents

`aurelius-mcp` makes Aurelius an MCP server. An outside app or agent (Claude Code, the
personal assistant) can then list projects, managers and tasks, create and follow tasks,
talk to a project's assistant, add ideas, and read and add memory. Each app has its own
**app key**. The server checks the key's scopes and limits on every call, and a revoked
key stops at once.

## 1. Make an app key

Do this in Aurelius under Settings, as an owner (the user on the server PC or a computer
paired as owner). Choose:

| Scope | Allows |
|---|---|
| `read` | projects, managers, tasks and their conversations, the inbox, ideas, memory |
| `draft` | tasks as drafts (no manager), ideas, memories |
| `run` | tasks that start at once, messages to tasks, stopping tasks, the project assistant (includes `draft`) |
| `approve` | Auto mode, answering approvals |

You can also set limits: tasks per hour, and tasks open at once (drafts do not count).

The secret (`amk_…`) is shown **once**. A task gets its manager's own mode, and runs in
Auto only when the key has `approve`. Without `approve`, a manager whose own mode is Auto
still gets the app's tasks in Ask. Every task shows the app that created it.

Besides scopes a key has an **audience** (feature `management_channel`, section 5) and
may have a **webhook** (feature `webhooks`, section 6). Both are set when the key is made.

Protocol (feature `app_keys`): `app_key.list`, `app_key.create`, `app_key.revoke`; the exact
shapes are in section 7.

## 2. Connect

### Locally (stdio, on the server PC)

```sh
claude mcp add aurelius --scope user -e AURELIUS_MCP_KEY=amk_… -- aurelius-mcp
```

or in `.mcp.json`:

```json
{
  "mcpServers": {
    "aurelius": {
      "command": "aurelius-mcp",
      "env": { "AURELIUS_MCP_KEY": "amk_…" }
    }
  }
}
```

`aurelius-mcp` connects to the Aurelius service on this PC. Use the full path of
`aurelius-mcp.exe` when it is not on the `PATH`; it is in the server release zip
(`aurelius-server-<version>-windows-x64.zip`). `--data-dir <dir>`
selects a data folder other than the default one.

### From another computer, over HTTP (the gateway)

Turn on remote access (Settings → Remote access, Tailscale). The gateway then serves MCP
over Streamable HTTP at `https://<pc>.<tailnet>.ts.net:8443/v1/mcp` (dev: port 8444).
Nothing needs to be installed on the other computer:

```sh
claude mcp add --transport http aurelius https://<pc>.<tailnet>.ts.net:8443/v1/mcp \
  --header "Authorization: Bearer amk_…"
```

or in `.mcp.json`:

```json
{
  "mcpServers": {
    "aurelius": {
      "type": "http",
      "url": "https://<pc>.<tailnet>.ts.net:8443/v1/mcp",
      "headers": { "Authorization": "Bearer ${AURELIUS_MCP_KEY}" }
    }
  }
}
```

### From another computer, over stdio

`aurelius-mcp` can also run on the other computer and reach the gateway's WebSocket:

```sh
claude mcp add aurelius -e AURELIUS_MCP_KEY=amk_… -e AURELIUS_MCP_URL=https://<pc>.<tailnet>.ts.net:8443 -- aurelius-mcp
```

## 3. Docker

The Docker image runs `aurelius-mcp --http 0.0.0.0:7464`: MCP over Streamable HTTP at
`http://<host>:7464/mcp`. Use it when the MCP client cannot reach the gateway itself (no
Tailscale on that machine, a server in a container network) or should see a fixed address
on your LAN. A Linux container cannot use the local connection to the Windows service, so
it always goes through the gateway: turn on remote access on the server first.

Each caller sends its own app key as `Authorization: Bearer amk_…`. The container passes it
on: every key gets its own session with the server, so scopes, limits and revoking work as
on the gateway. A revoked key closes its session at once, and the next request gets `401`.
Without a key the answer is `401`. `AURELIUS_MCP_KEY` is optional: it serves callers that
send no `Authorization` header. Anyone who can reach port 7464 then acts as that app, so
only set it on a network you trust. The container itself speaks plain HTTP: keep port 7464
on your LAN or tailnet, and put a TLS proxy in front of it if it must go further.

Every server release publishes the image as `ghcr.io/segrusa/aurelius-mcp:<version>`
and `:latest` (linux/amd64). Start it:

```sh
docker run -d --name aurelius-mcp -p 7464:7464 \
  -e AURELIUS_MCP_URL=https://<pc>.<tailnet>.ts.net:8443 ghcr.io/segrusa/aurelius-mcp:latest
```

or with `docker-compose.yml` (set the variables in your shell or in a `.env` file next to it):

```sh
AURELIUS_MCP_URL=https://<pc>.<tailnet>.ts.net:8443 docker compose up -d
```

To build the image from a checkout instead, run `docker compose up -d --build` or
`docker build -t aurelius-mcp .` from the repository root.

Then connect the client:

```sh
claude mcp add --transport http aurelius http://<host>:7464/mcp \
  --header "Authorization: Bearer amk_…"
```

`GET` and `DELETE` on `/mcp` answer `405`, and requests with an `Origin` header (a web page)
get `403`, as on the gateway. The image is a release build, so it only talks to production
servers; set `AURELIUS_ENV=dev` to reach a dev server (ports 8444 and 47614).

### How the container reaches the gateway

- **Through Tailscale (recommended):** `AURELIUS_MCP_URL=https://<pc>.<tailnet>.ts.net:8443`
  (dev: 8444). Tailscale must run on the Docker host, or the host must otherwise route to the
  tailnet. The container checks the gateway's certificate (Let's Encrypt, issued by Tailscale
  for the ts.net name) with the CA certificates in the image, so the URL must use that exact
  ts.net name; a tailnet IP address does not match the certificate. When the container cannot
  resolve the name (MagicDNS is not passed into Docker), map it to the server's tailnet address
  and keep the name in the URL:
  `--add-host <pc>.<tailnet>.ts.net:100.x.y.z` (compose: under `extra_hosts`).
- **Docker on the server PC itself:** `AURELIUS_MCP_URL=http://host.docker.internal:47613`
  (dev: 47614). This is the gateway's own loopback port behind Tailscale Serve: plain HTTP,
  no TLS and no certificate, and the traffic does not leave the PC. `aurelius-mcp` allows plain
  HTTP only to loopback addresses and `host.docker.internal`. Remote access must still be on,
  or the gateway does not run. `https://host.docker.internal:8443` does not work: Tailscale
  Serve listens on the tailnet address, and its certificate is for the ts.net name.

## 4. The personal assistant

The assistant runs Claude Code as a child process. Give it Aurelius like any MCP
server in its MCP config:

- on the same PC: the stdio entry above;
- elsewhere on the tailnet: the HTTP entry above;
- without Tailscale on its machine: the Docker container's `http://<host>:7464/mcp`.

A good key for the assistant is `read` + `draft` + `run`, with limits (for example
10 per hour, 3 at once), and without `approve`. It creates tasks only after the user
says yes. Approvals stay with the user in Aurelius.


## 5. Audience: the management channel

Feature `management_channel`. An app key has an `audience`:

| Audience | Reaches |
|---|---|
| `all` (default) | Everything its scopes allow. Every key made before this feature is `all` and behaves exactly as before. |
| `management` | Only the **management**: the CEO and the employees who report to the CEO directly. |

**Who the CEO is.** The root of the org chart whose role or title names the CEO ("CEO", "Chief
Executive Officer"); without such a title, the oldest root. The management is that employee plus
their direct reports (archived employees do not count), taken from the org chart at the time of
each call, so a change in the chart applies at once.

**The server enforces it**, on every request of the key's session (local IPC, the gateway
WebSocket, `/v1/mcp` and the Docker container alike), not only in the `aurelius-mcp` tools. For a
`management` key:

| What | Behaviour |
|---|---|
| `employee.list` (`list_employees`, alias `list_managers`) | Only the CEO and the CEO's direct reports. |
| `task.list` (`list_tasks`, `read_task`) | Only tasks of the management and tasks this app created. |
| `read_inbox`, approvals | Only what concerns the management; inbox items without a task are left out. |
| Create a task **for** an employee outside the management | Refused (`forbidden`): "This app key reaches the management only (the CEO and the CEO's direct reports). Ask one of them to pass the work on." A draft (no employee) is fine. |
| Message an employee outside the management (`message_task`, `ask_management`) | Same refusal. |
| Stop a task of an employee outside the management | Same refusal. |
| The project assistant (`ask_project_assistant`, project conversations, `conversation.get` on them) | Refused: "This app key reaches the management only; it cannot talk to a project's assistant. Use ask_management." |
| The org chart and departments (`company.get`, `department.list`) | Refused. |
| Read a manager's persistent chat | Only a conversation this app started with `ask_management`; any other chat is refused ("Apps only read the conversations they started with ask_management."). The user's own chat with a manager is never visible. |
| The `approve` scope | Not allowed on a `management` key: `app_key.create` fails with a validation error. Auto mode and approvals stay with the user. |
| Answering a manager's question | Needs only `run` (see `answer_question`), and only for questions of the management. |

Errors are `forbidden` protocol errors; over MCP a tool answers with `isError: true` and the same
text. A tool that a key cannot use is not listed in `tools/list`.

**Conversations.** `ask_management` writes in the manager's persistent chat (the same chat the user
sees in Aurelius, so the manager keeps its context), marked in the text as coming from the app.
The reply is a normal manager answer: it takes as long as a run takes, hence asynchronous.

## 6. Webhooks

Feature `webhooks`. A key with a `webhook_url` gets its events posted there; Aurelius, not the
app, initiates the call (use it when the app runs on another machine and cannot hold a
connection open).

### URL rules

- `http://` or `https://` only (`ftp:`, `file:`, `javascript:` and the like are refused). Plain
  `http` is fine, for a receiver on your LAN or tailnet: `http://home-server:8787/hooks/aurelius`.
- A host is required; a port is optional (1-65535); a path and query are fine; **no** user name
  or password in the URL and **no** `#fragment`; no spaces; at most 2000 characters.
- Redirects are **not** followed (a 3xx counts as a failed attempt): give the final URL.
- Validated at `app_key.create`; an invalid URL fails the call (`validation`).
- Aurelius connects from the server PC, so the URL must be reachable from there. HTTPS
  certificates are checked normally.

### The secret

When a key is created with a `webhook_url`, the answer has `webhook_secret` (`whsec_` + 64 hex
characters). It is shown **once**: not in `app_key.list`, not in any later answer, not in the
receipt of the write. Aurelius has to keep it (in its database) to sign deliveries. To change the
URL or the secret, revoke the key and make a new one. The secret is separate from the key's own
`amk_...` secret.

### Events

An app gets events about **its own work** (tasks it created and conversations it started); a
`management` app also gets events about the management's tasks and questions. Never events about
other employees or other apps.

| Event | When | Sent to |
|---|---|---|
| `manager.replied` | A manager answered a message the app sent with `ask_management` (once per message, even when the reply came late). | The app that wrote in that conversation. |
| `question.for_you` | A manager asks the user a question or a decision (Claude's `AskUserQuestion`). Answer with `answer_question`. | Apps whose task or conversation it is; management apps for any management question. |
| `task.finished` | A task ended: `succeeded`, `failed` or `cancelled`. | Apps that created the task; management apps for tasks of the management. |
| `task.waiting` | A task waits (see `reason`). | Same as `task.finished`. |

Plans and tool approvals (things the user approves) are **not** events: they stay in Aurelius.

### The request

`POST <webhook_url>` with a JSON body (`Content-Type: application/json`), a `User-Agent` of
`Aurelius/<version>`, and these headers:

| Header | Value |
|---|---|
| `X-Aurelius-Delivery` | The delivery id, `dlv_` + 32 hex characters. **The same on every retry** of the same delivery. |
| `X-Aurelius-Event` | The event name, e.g. `task.finished`. |
| `X-Aurelius-Timestamp` | Unix time in **seconds** at which this attempt was sent. New on every attempt. |
| `X-Aurelius-Attempt` | `1` for the first attempt, `2` for the first retry, ... |
| `X-Aurelius-Signature` | `v1=` + lowercase hex of the HMAC-SHA256, see below. |

Body (the same bytes on every attempt of one delivery):

```json
{
  "id": "dlv_3f2a9c1e5b7d4a60918c2d4e6f8a0b1c",
  "type": "task.finished",
  "created_at_ms": 1766400000000,
  "app": { "id": "0192e5c4-1d2a-7b3c-8d4e-5f6a7b8c9d0e", "name": "Marcus" },
  "data": { }
}
```

`id` equals `X-Aurelius-Delivery`; `type` equals `X-Aurelius-Event`; `created_at_ms` is when the
event happened. `data` per event:

`manager.replied`:

```json
{
  "conversation_id": "0192e5c4-0000-7000-8000-00000000c0de",
  "message_id": "0192e5c4-1111-7000-8000-00000000a001",
  "employee_id": "0192e5c4-0000-7000-8000-0000000000e1",
  "employee": "Jean",
  "text": "The plan: ship on Friday, after the security review."
}
```

`question.for_you` (`id` is what you pass to `answer_question`):

```json
{
  "id": "0192e5c4-2222-7000-8000-00000000b002",
  "kind": "question",
  "employee_id": "0192e5c4-0000-7000-8000-0000000000e1",
  "employee": "Jean",
  "task_id": "0192e5c4-3333-7000-8000-00000000c003",
  "conversation_id": "0192e5c4-0000-7000-8000-00000000c0de",
  "questions": [
    { "question": "Ship on Friday or wait for the audit?", "header": "Release",
      "options": [ { "label": "Friday", "description": "Ship now" }, { "label": "Wait", "description": "After the audit" } ],
      "multiSelect": false }
  ],
  "expires_at_ms": 1766486400000
}
```

(`task_id` and `conversation_id` are `null` when they do not apply; `questions` is Claude's
`AskUserQuestion` input as is.)

`task.finished`:

```json
{
  "task_id": "0192e5c4-3333-7000-8000-00000000c003",
  "title": "Plan the launch",
  "status": "succeeded",
  "project_id": "0192e5c4-4444-7000-8000-00000000d004",
  "project": "Atlas",
  "employee_id": "0192e5c4-0000-7000-8000-0000000000e1",
  "employee": "Jean",
  "conversation_id": "0192e5c4-5555-7000-8000-00000000e005",
  "created_by_this_app": true,
  "report": "Launch plan: ..."
}
```

`status` is `succeeded`, `failed` or `cancelled`; `report` is the task's report (cut at 6000
characters) or `null`.

`task.waiting` (same fields as `task.finished`, without `report`):

```json
{
  "task_id": "0192e5c4-3333-7000-8000-00000000c003",
  "title": "Plan the launch",
  "status": "waiting",
  "reason": "quota",
  "until_ms": 1766403600000,
  "project_id": "0192e5c4-4444-7000-8000-00000000d004",
  "project": "Atlas",
  "employee_id": "0192e5c4-0000-7000-8000-0000000000e1",
  "employee": "Jean",
  "conversation_id": "0192e5c4-5555-7000-8000-00000000e005",
  "created_by_this_app": true
}
```

`reason` is a short code: `delegated` (waits for work it gave to a colleague), `quota` (usage
limit; `until_ms` says when it resumes), `escalated_to_boss`, `variants`, `stopped` or
`not_started` (stopped and nobody above can take it: it waits for the user). `until_ms` is `null`
unless `quota`. Treat an unknown `reason` as informational.

### The signature

Algorithm **HMAC-SHA256** with the webhook secret (the whole `whsec_...` string, as UTF-8 bytes) as
the key. The string to sign is

```
{X-Aurelius-Timestamp}.{X-Aurelius-Delivery}.{raw request body}
```

(the three parts joined by single dots; the body is the raw bytes as received, not re-serialized).
`X-Aurelius-Signature` is `v1=` followed by the lowercase hex of the 32-byte MAC. To verify:

1. Read the raw body, the timestamp and the delivery id from the request.
2. Refuse (400/401) when the timestamp is more than **300 seconds** away from your clock (either
   way). Each attempt has a fresh timestamp, so a retry hours later still passes.
3. Compute the HMAC and compare with the header in **constant time**.
4. Only then parse the JSON.

Node.js:

```js
const crypto = require("node:crypto");
function verify(secret, headers, rawBody, nowSecs = Date.now() / 1000) {
  const ts = Number(headers["x-aurelius-timestamp"]);
  if (!Number.isFinite(ts) || Math.abs(nowSecs - ts) > 300) return false;
  const signed = `${ts}.${headers["x-aurelius-delivery"]}.${rawBody}`;
  const expected = "v1=" + crypto.createHmac("sha256", secret).update(signed).digest("hex");
  const given = String(headers["x-aurelius-signature"] || "");
  return given.length === expected.length && crypto.timingSafeEqual(Buffer.from(given), Buffer.from(expected));
}
```

Python:

```python
import hmac, hashlib, time
def verify(secret: str, headers: dict, raw_body: bytes, now=None) -> bool:
    ts = int(headers["x-aurelius-timestamp"])
    if abs((now or time.time()) - ts) > 300:
        return False
    signed = f'{ts}.{headers["x-aurelius-delivery"]}.'.encode() + raw_body
    expected = "v1=" + hmac.new(secret.encode(), signed, hashlib.sha256).hexdigest()
    return hmac.compare_digest(headers["x-aurelius-signature"], expected)
```

### Delivery, success and retries

- **Success** is any `2xx` answer. Answer quickly (within the timeout) and do the real work
  afterwards. The body of your answer is ignored.
- **Everything else is a failure and is retried**: any other status (also `3xx`, `4xx`, `5xx`), a
  connection error, or no answer within **10 seconds** (connect timeout 5 s).
- **Schedule.** At most **8 attempts** in all (the first plus 7 retries). The wait after a failed
  attempt doubles: 5 s, 10 s, 20 s, 40 s, 80 s, 160 s, 320 s (so the last attempt comes about 10
  minutes after the first). After the 8th failure the delivery is given up (logged on the server,
  kept for a week, never sent again).
- **Persistent.** Deliveries wait in a database outbox from the moment the event happens; a server
  restart loses none, and overdue deliveries go out right after the start. While the server is
  off nothing is sent. A delivery whose key was revoked is dropped.
- **Order.** Deliveries are attempted oldest first but not strictly in order: a retry can arrive
  after a newer event. Use `created_at_ms` and the task `status` to settle conflicts.

### Idempotency

The delivery id (`X-Aurelius-Delivery`, also `id` in the body) is stable across retries and unique
per event and app. Delivery is **at least once**: a receiver may see the same id twice (for
example when its `2xx` got lost). Store the ids you handled and ignore a repeat (answer `2xx`
again). Never act twice on the same id.

## 7. Protocol shapes (for clients: Web, Mobile, desktop)

Both features are additive to protocol 3.0; every new field has a default. They appear in
`HelloResult.features` as `management_channel` and `webhooks`.

`app_key.create` (owner only; a host write), request `params`:

```json
{
  "name": "Marcus",
  "scopes": ["read", "run"],
  "limit_per_hour": 20,
  "limit_concurrent": 3,
  "audience": "management",
  "webhook_url": "http://home-server:8787/hooks/aurelius"
}
```

`audience` (`"all"` | `"management"`, default `"all"`) and `webhook_url` (string | `null`, default
`null`; empty or blank counts as `null`) are the new fields. `limit_*` stay optional. Validation:
`management` with the `approve` scope is refused; a bad `webhook_url` is refused (`validation`).

Response (`HostResult` `app_key_created`, i.e. `AppKeyCreated`):

```json
{
  "key": {
    "id": "0192e5c4-1d2a-7b3c-8d4e-5f6a7b8c9d0e",
    "name": "Marcus",
    "scopes": ["read", "run"],
    "limit_per_hour": 20,
    "limit_concurrent": 3,
    "created_at_ms": 1766400000000,
    "last_used_ms": null,
    "revoked_at_ms": null,
    "tasks_last_hour": 0,
    "tasks_open": 0,
    "audience": "management",
    "webhook_url": "http://home-server:8787/hooks/aurelius"
  },
  "secret": "amk_...",
  "webhook_secret": "whsec_..."
}
```

`secret` and `webhook_secret` are shown once; `webhook_secret` is absent (not `null`) when the key
has no webhook.

`app_key.list` returns `app_keys`: an array of the `key` objects above (new: `audience`,
`webhook_url`; never a secret). `app_key.revoke` answers one `key`. A key made before this feature
lists `"audience": "all"` and `"webhook_url": null`. `HelloResult.app` (`AppSession`) gains
`audience`.

There are no new protocol methods and no new events: the MCP tools use existing methods
(`employee.send_message`, `approval.list`, `task.list`, `notification.list`,
`approval.answer`, `conversation.get`).

## 8. Feature flags

| Flag (`HelloResult.features`) | Turns on |
|---|---|
| `app_keys` | App keys, scopes, the MCP server (unchanged). |
| `management_channel` | `audience` on keys and sessions; the tools `ask_management`, `list_open_questions`, `answer_question`. |
| `webhooks` | `webhook_url` / `webhook_secret`, the outbox and the signed events. |

A client checks `features` before showing the fields (an older server does not know them and
ignores them). `aurelius-mcp` lists the three tools only when the server has `management_channel`.
A server of this version always has both flags.

## 9. Tools

Every tool call is a protocol call by the app's session; the server checks scope, limits,
revocation and audience. `Scope` is what the key needs. The tools marked **M** exist for
`management` keys only; **A** are for `all` keys only.

| Tool | Scope | Protocol |
|---|---|---|
| `list_projects` | read | `project.list` |
| `list_employees` (alias `list_managers`) | read | `employee.list` |
| `list_tasks`, `read_task` | read | `task.list`, `conversation.get` |
| `read_conversation` | read | `conversation.get` (project conversations; for `management` keys also the conversations they started with `ask_management`) |
| `read_inbox` | read | `notification.list`, `approval.list` |
| `list_open_questions` | read | `approval.list`, `task.list`, `notification.list` |
| `recall` | read | `memory.list` |
| `create_task` | draft / run | `task.create` (with an employee it starts: run; for **M** keys JSON, see above) |
| `add_idea`, `remember` | draft | `idea.add`, `memory.remember` |
| `message_task`, `stop_task` | run | `employee.send_message`, `task.cancel` |
| `ask_project_assistant` (**A**) | run | `project_conversation.send`, then `conversation.get` until the answer is there |
| `ask_management` (**M**) | run | `employee.send_message` (the manager's persistent chat), `conversation.get` |
| `answer_question` (**M**) | run | `approval.answer`, or `employee.send_message` for a waiting task |
| `set_task_mode` (**A**) | approve | `conversation.set_agent_mode` (Auto needs approve) |
| `answer_approval` (**A**) | approve | `approval.decide`, `approval.decide_plan`, `approval.answer` |

An app never sees the user's own chats (a `management` app sees only the conversations it
started), terminals, settings or other keys. It cannot attach files from the server, and it cannot
choose "always allow". `tools/list` shows only the tools a key can use.

The three management tools return **JSON text** (one `text` content item) so a program can parse
them. On failure `isError` is `true` and the text is a plain sentence.

### `ask_management`

Starts or continues the app's conversation with a manager. The manager answers asynchronously.

Input:

| Field | Type | |
|---|---|---|
| `message` | string, required | The question or the decision to make. |
| `to` | string | `CEO`, `CTO` or `CMO` (matched on the employee's role or title), or a manager's name or id. Default `CEO`. Only the management can be reached. |
| `project` | string | A project (name or id) this is about; the manager is told. Optional. |
| `kind` | `question` (default) or `decision` | `decision` asks the manager for a clear decision and a reason. |
| `wait_seconds` | integer, default 0, max 600 | Wait for the reply in this call. 0: return at once; the reply comes as `manager.replied` or via `read_conversation`. |

Output:

```json
{
  "status": "sent",
  "kind": "question",
  "to": { "id": "...", "name": "Jean", "role": "Management", "title": "CEO" },
  "project": { "id": "...", "name": "Atlas" },
  "conversation_id": "...",
  "message_id": "...",
  "run_id": "...",
  "reply": null
}
```

With `wait_seconds` and an answer in time, `status` is `answered` and `reply` is
`{ "text": "...", "status": "complete" | "interrupted" | "failed" }`. Calling it again with the same
manager continues the same conversation (the manager's persistent chat; its history is kept).
Errors: `message` missing; unknown `kind`; no manager matches `to` (or several do: use the name);
the manager is paused or archived; the key lacks `run`; the key is not a `management` key.

### `list_projects` and `create_task` for a management key

Same tools as for every key, but a `management` key gets **JSON text** back, so an app can fill a
picker and follow the task. Everything stays inside the management: the task goes to the CEO or one
of the CEO's direct reports, never to anyone else (`forbidden` otherwise).

`list_projects` output:

```json
{
  "projects": [ { "id": "...", "name": "Atlas", "description": "...", "employees_active": 2 } ],
  "task_modes": ["ask", "accept_edits", "plan"]
}
```

`task_modes` are the modes `create_task` accepts for this key: `ask`, `accept_edits`, `plan`, and
`auto` **only** with the `approve` scope (a `management` key never has it, so it is not listed).
An app should take the modes from here and not hard-code them.

`create_task` input: `project` (name or id, required), `title` (required), `goal`, `criteria`,
`employee` (`CEO`, `CTO`, `CMO`, a name or an id; default `CEO`), `mode` (one of `task_modes`;
empty: the manager's own mode), `start` (default `true` for a management key with `run`).
A mode outside `task_modes` is refused with a plain sentence. Output:

```json
{
  "status": "queued",
  "task_id": "...",
  "title": "Plan the launch",
  "project": { "id": "...", "name": "Atlas" },
  "employee": { "id": "...", "name": "Jean", "role": "Management", "title": "CEO" },
  "mode": "plan"
}
```

`status` is `queued` (it starts when the manager is free) or `draft` (`start: false`; the user starts
it in Aurelius); `mode` is `null` when the manager's own mode applies. Follow the task with
`task.finished` / `task.waiting` webhooks, `read_task` or `message_task` (`task_id`).

### Threads in an app (how Marcus links answers)

An app that shows threads (Marcus does) keeps the link itself; Aurelius needs nothing extra:

- A reply in a thread about a **chat** goes through `ask_management` again: a manager has one persistent
  chat per app, so it is the same conversation (`conversation_id` is stable and returned every time).
- A reply in a thread about a **task** goes through `message_task` with the `task_id` that `create_task`
  returned.
- `manager.replied` carries `conversation_id` only, so the app files it under the thread of the last message
  it sent into that conversation. `question.for_you` and `task.finished` / `task.waiting` carry `task_id`
  (and `conversation_id`), which maps to the thread of that task.

### `list_open_questions`

What waits for the user. Input: `project` (optional filter by project). Output:

```json
{
  "questions": [
    { "id": "<approval id>", "type": "question", "from": "Jean", "task_id": null, "conversation_id": "...",
      "requested_at_ms": 1766400000000, "expires_at_ms": 1766486400000,
      "questions": [ { "question": "...", "header": "...", "options": [ { "label": "...", "description": "..." } ], "multiSelect": false } ] }
  ],
  "waiting_tasks": [
    { "id": "<task id>", "title": "...", "status": "waiting", "reason": "not_started", "employee": "Cora",
      "project_id": "...", "pending_approvals": 0, "updated_at_ms": 1766400000000 }
  ],
  "needs_you": [
    { "id": "...", "title": "...", "subject": "...", "task_id": "...", "created_at_ms": 1766400000000 }
  ]
}
```

`questions`: questions and decisions of managers to the user (Claude's `AskUserQuestion`);
`waiting_tasks`: tasks in status `waiting` or `review`; `needs_you`: unread inbox items that need
the user. A `management` key sees only the management's items.

### `answer_question`

Sends the answer back to the manager who asked.

| Field | Type | |
|---|---|---|
| `id` | string, required | A `questions[].id` (or its first 8 characters), or the `id` of a waiting task. |
| `answer` | string | The answer. For a question with several parts it answers all of them. |
| `answers` | object | Several parts: the question text mapped to its answer (overrides `answer`). |

Output: `{ "answered": "question", "id": "<approval id>", "from": "Jean" }`: the answer reached
the running manager, which continues. For a waiting task `id`:
`{ "answered": "task_message", "id": "<task id>", "employee": "Cora", "conversation_id": "..." }`:
the answer was sent to the task's employee as a message. Errors: `id` or `answer` missing; no open
question or waiting task matches; the question is not from the management ("That question is not
from the management."); the question was already answered or has expired.
