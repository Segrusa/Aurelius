// The vault (Fase 4) without widgets: names, validation, the kinds an item
// may change to (B7), the audit log's lines and the fixed texts (E21, B10).

import 'protocol/protocol.dart';

/// R2 (E21): what the vault screen says about plugins and MCP servers.
const vaultPluginsNotice = 'Plugins and MCP servers of an employee see the secrets of its runs.';

/// R3 (E21): what the vault screen says about the key on the server.
const vaultKeyNotice = "On Linux, any process of the daemon's user, also an agent through Bash, can read the key.";

/// Under the value of a text or url item (m9): only a secret is shielded and redacted.
const vaultNotSecretHint = 'Not secret: shown to everyone and not redacted. Use Secret for keys and tokens.';

/// Who can read an item, under the editor (ui §4).
const vaultReadersNotice = 'All employees can read this item. Secrets reach runs only as an environment variable, never in a prompt, memory or discussion.';

/// The key export is a desktop action (B8, E9: it asks for the password again and saves a file).
const vaultKeyExportNotice = 'To keep the vault key for a restore on another machine, export it on the desktop.';

/// How long a shown secret stays visible, and when a copied one leaves the clipboard (B6, P-47).
const revealFor = Duration(seconds: 30);
const clipboardClearAfter = Duration(seconds: 60);

/// The shortest secret the server takes (E13): a shorter one could not be redacted.
const minSecretLength = 8;

final _envName = RegExp(r'^[A-Z][A-Z0-9_]{0,63}$');

/// The key of an item as a run sees it: UPPER_SNAKE, starting with a letter.
bool validEnvName(String name) => _envName.hasMatch(name);

/// The key the editor suggests for [name] ("GitHub token" → `GITHUB_TOKEN`).
String envNameFrom(String name) {
  final snake = name.trim().toUpperCase().replaceAll(RegExp(r'[^A-Z0-9]+'), '_').replaceAll(RegExp(r'^_+|_+$'), '');
  final lead = snake.replaceFirst(RegExp(r'^[0-9_]+'), '');
  return lead.length > 64 ? lead.substring(0, 64) : lead;
}

/// A url item's value: a full `https://` or `http://` address.
bool validVaultUrl(String value) {
  final uri = Uri.tryParse(value.trim());
  return uri != null && (uri.scheme == 'https' || uri.scheme == 'http') && uri.host.isNotEmpty;
}

/// Why [value] cannot be saved as [kind], or null. [keep]: an existing secret
/// whose value is left as it is (the field may stay empty).
String? vaultValueError(VaultKind kind, String value, {bool keep = false}) {
  if (value.isEmpty) return keep ? null : 'Enter a value.';
  return switch (kind) {
    VaultKind.url when !validVaultUrl(value) => 'Enter a full URL, starting with https://',
    VaultKind.secret when value.trim().length < minSecretLength => 'A secret has at least $minSecretLength characters.',
    _ => null,
  };
}

/// Why [envName] cannot be the key, or null. [taken]: the keys of the other items in the scope.
String? envNameError(String envName, Iterable<String> taken) {
  if (envName.isEmpty) return 'Enter a key.';
  if (!validEnvName(envName)) return 'A–Z, 0–9 and _, starting with a letter.';
  if (taken.contains(envName)) return 'Another item in this scope has this key.';
  return null;
}

/// The kinds an item may be saved as. A new item: any. An existing one:
/// text and url change into each other; a secret stays a secret and
/// nothing becomes one (B7, P-44).
List<VaultKind> kindChoices(VaultKind? current) => switch (current) {
      null => const [VaultKind.text, VaultKind.url, VaultKind.secret],
      VaultKind.secret => const [VaultKind.secret],
      _ => const [VaultKind.text, VaultKind.url],
    };

String kindLabel(VaultKind kind) => switch (kind) {
      VaultKind.text => 'Text',
      VaultKind.url => 'URL',
      VaultKind.secret => 'Secret',
    };

/// The masked value: the same length for every secret, so nothing leaks.
const maskedValue = '••••••••';

/// How a screen reader reads a hidden secret (ui §6).
String hiddenLabel(VaultItemView item) => '${item.name}, secret, hidden';

/// The item the project's runs get instead of a company item with the same key (B4).
bool overridesCompany(VaultItemView item) => item.overridesCompany ?? false;

/// "Delete GitHub token?" and what it means (ui §4, MU4-11 AC2).
String archiveQuestion(VaultItemView item) => 'Delete ${item.name}?';
String archiveDetail(VaultItemView item) =>
    'Runs and employees lose access: new runs no longer get ${item.envName}. Runs that already have it keep it until they end. This is logged.';

/// The confirmation before Show and Copy of a secret (MU4-10 AC2). Only a
/// confirmation: B6 rules out a password or biometrics.
String showQuestion(VaultItemView item) => 'Show ${item.name}?';
String showDetail() => 'Shows the value for ${revealFor.inSeconds} s. This is logged.';
String copyQuestion(VaultItemView item) => 'Copy ${item.name}?';
String copyDetail() => 'Copies the value. The clipboard clears in ${clipboardClearAfter.inSeconds} s. This is logged.';

/// The warning when the key of an item changes (ui §4).
String envRenameWarning(String old) => 'Runs that use $old will no longer find it.';

/// The B10 question for a message that holds the value of [items] (P-46).
/// The PC stores it with each value replaced by `«secret:KEY»`, the item's key.
const secretCheckTitle = 'This looks like a vault secret. Send anyway?';
String secretCheckBody(List<String> items) {
  final names = items.isEmpty ? 'a vault item' : items.join(', ');
  return 'It holds the value of $names. If you send it anyway, the value is replaced by «secret:KEY», with the item\'s key, before it is stored.';
}

/// B10 where "Send anyway" cannot help (M-m5, m8): a value in an attached
/// file, or a command the PC never stores redacted. Sent again it is
/// refused again, so the only way on is to take the value out.
const secretCheckNoSendAnyway = 'Take the value out of the message or file.';

/// The B10 question where "Send anyway" cannot help: what to take out ([takeOut]).
const secretHeldTitle = 'This looks like a vault secret';
String secretHeldBody(List<String> items, String takeOut) => 'It holds the value of ${items.isEmpty ? 'a vault item' : items.join(', ')}. $takeOut';

/// An audit row's action as a verb ("revealed"), for "You revealed GitHub token".
String auditVerb(VaultAuditAction action) => switch (action) {
      VaultAuditAction.create => 'created',
      VaultAuditAction.edit => 'edited',
      VaultAuditAction.replace => 'replaced',
      VaultAuditAction.reveal => 'revealed',
      VaultAuditAction.copy => 'copied',
      VaultAuditAction.deliver => 'got as an environment variable',
      VaultAuditAction.toolRead => 'read with the vault tool',
      VaultAuditAction.archive => 'deleted',
      VaultAuditAction.keyExport => 'exported the vault key',
    };

/// The action's name in the filter (ui §5).
String auditActionLabel(VaultAuditAction action) => switch (action) {
      VaultAuditAction.create => 'Created',
      VaultAuditAction.edit => 'Edited',
      VaultAuditAction.replace => 'Replaced',
      VaultAuditAction.reveal => 'Revealed',
      VaultAuditAction.copy => 'Copied',
      VaultAuditAction.deliver => 'Read by run',
      VaultAuditAction.toolRead => 'Read with the tool',
      VaultAuditAction.archive => 'Deleted',
      VaultAuditAction.keyExport => 'Key exported',
    };

/// Who did it: "You" for this phone's board member, else the name (an employee with "(employee)").
String auditWho(VaultActor actor, {String? me}) => switch (actor) {
      VaultActorBoardMember(:final user) => user.id == me ? 'You' : user.username,
      VaultActorEmployee(:final name) => '$name (employee)',
    };

/// "You revealed GitHub token", "Forge (employee) got GITHUB_TOKEN as an
/// environment variable", "Robin edited Staging URL (name, key)". Never a value.
String auditLine(VaultAuditRow row, {String? me}) {
  final who = auditWho(row.actor, me: me);
  if (row.action == VaultAuditAction.keyExport) return '$who ${auditVerb(row.action)}';
  final item = row.itemName ?? 'an item';
  final line = switch (row.action) {
    VaultAuditAction.deliver => '$who got $item as an environment variable',
    VaultAuditAction.toolRead => '$who read $item with the vault tool',
    _ => '$who ${auditVerb(row.action)} $item',
  };
  if (row.action != VaultAuditAction.edit || row.changedFields.isEmpty) return line;
  return '$line (${row.changedFields.map(_fieldName).join(', ')})';
}

String _fieldName(String field) => switch (field) {
      'env_name' => 'key',
      'description' => 'note',
      'kind' => 'type',
      _ => field,
    };

/// Under the line: where it happened (the board member's device, or the run).
String? auditVia(VaultAuditRow row) => switch (row.actor) {
      VaultActorBoardMember() => row.via,
      VaultActorEmployee(:final ticketId) when ticketId != null => 'A run of a ticket',
      VaultActorEmployee(:final runId) when runId != null => 'A run',
      VaultActorEmployee() => null,
    };

/// The rows the filters let through (the server filters scope and item; who and action are the phone's).
List<VaultAuditRow> filterAudit(List<VaultAuditRow> rows, {VaultAuditAction? action, AuditWho who = AuditWho.anyone}) => [
      for (final r in rows)
        if ((action == null || r.action == action) &&
            switch (who) {
              AuditWho.anyone => true,
              AuditWho.boardMembers => r.actor is VaultActorBoardMember,
              AuditWho.employees => r.actor is VaultActorEmployee,
            })
          r,
    ];

enum AuditWho { anyone, boardMembers, employees }
