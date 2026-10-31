/// Unsent text of the composers (ui-ontwerp Fase 3 §1, R3 M1): kept outside
/// the widget tree, so a `signed_out` that sends the app to the sign-in
/// screen does not take a half-written message with it. In memory only (no
/// disk: a draft may hold anything), per host, board member and conversation
/// or ticket, so another board member on the same phone never sees it.
class DraftStore {
  DraftStore();

  /// The phone's one store: it outlives a host's app and its pages.
  static final instance = DraftStore();

  final _drafts = <(String, String, String), String>{};

  /// [userId]'s draft in [slot] on [hostId]; null without one.
  String? read(String hostId, String userId, String slot) => _drafts[(hostId, userId, slot)];

  /// Keep [text] as the draft; an empty text (sent, or wiped) forgets it.
  void write(String hostId, String userId, String slot, String text) {
    if (text.trim().isEmpty) {
      _drafts.remove((hostId, userId, slot));
    } else {
      _drafts[(hostId, userId, slot)] = text;
    }
  }

  /// Forget everything (a removed host, tests).
  void clear() => _drafts.clear();
}
