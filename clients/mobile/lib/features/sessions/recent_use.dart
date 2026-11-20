// When you last opened a session or a ticket on this phone, so the Sessions
// tab lists them most recently used first: the server only knows their
// activity, not that you looked at one. Ported from the desktop's
// recent-use-logic.ts.

import 'dart:convert';

import 'package:shared_preferences/shared_preferences.dart';

import '../../data/aurelius_repository.dart';

/// How many open times are kept; the oldest go first.
const recentUseMax = 100;

/// Where the open times are kept, per server.
String recentUseKey(String hostId) => 'sessions.used:$hostId';

String sessionUseKey(String conversationId) => 'session:$conversationId';
String ticketUseKey(String ticketId) => 'ticket:$ticketId';

/// The stored open times; anything unreadable counts as none, so a bad value never breaks the list.
Map<String, int> parseUses(String? raw) {
  if (raw == null || raw.isEmpty) return {};
  try {
    final value = jsonDecode(raw);
    if (value is! Map) return {};
    return {
      for (final e in value.entries)
        if (e.key is String && e.value is num && (e.value as num).isFinite) e.key as String: (e.value as num).toInt(),
    };
  } on FormatException {
    return {};
  }
}

/// [uses] with [key] opened at [now], keeping only the latest [max].
Map<String, int> touchUse(Map<String, int> uses, String key, int now, {int max = recentUseMax}) {
  final entries = ({...uses, key: now}).entries.toList()..sort((a, b) => b.value.compareTo(a.value));
  return Map.fromEntries(entries.take(max));
}

/// When an item was last used: its own activity, or when you last opened it here, whichever is later.
int lastUsed(Map<String, int> uses, String key, int activityMs) {
  final opened = uses[key] ?? 0;
  return opened > activityMs ? opened : activityMs;
}

/// Reads the open times of [hostId]; none when the store cannot be read.
Future<Map<String, int>> loadUses(String hostId) async {
  try {
    final prefs = await SharedPreferences.getInstance();
    return parseUses(prefs.getString(recentUseKey(hostId)));
  } catch (_) {
    return {};
  }
}

/// Notes that [key] was opened now on [hostId]; a store that fails only loses the order.
Future<void> noteUse(String hostId, String key) async {
  try {
    final prefs = await SharedPreferences.getInstance();
    final uses = touchUse(parseUses(prefs.getString(recentUseKey(hostId))), key, DateTime.now().millisecondsSinceEpoch);
    await prefs.setString(recentUseKey(hostId), jsonEncode(uses));
  } catch (_) {
    // Not saved: the list falls back to the activity alone.
  }
}

/// The server [repository] talks to, for the store's key; null when it is not known.
String? _hostOf(AureliusRepository repository) {
  try {
    return repository.host.hostId;
  } on Object {
    return null;
  }
}

/// [noteUse] on [repository]'s server.
Future<void> noteUseOn(AureliusRepository repository, String key) async {
  final host = _hostOf(repository);
  if (host != null) await noteUse(host, key);
}

/// [loadUses] of [repository]'s server.
Future<Map<String, int>> loadUsesOn(AureliusRepository repository) async {
  final host = _hostOf(repository);
  return host == null ? {} : loadUses(host);
}
