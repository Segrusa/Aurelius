// Wire format shared with the desktop (crates/protocol), generated into
// lib/data/protocol. One JSON object per WebSocket text frame. Requests carry
// `protocol` (the major), `request_id`, `method` and optional `params`; the
// daemon answers with a `response` and pushes `event`, `stream_append`,
// `load_changed` and `power_changed`.

import '../../data/protocol/protocol.dart';

export '../../data/protocol/protocol.dart' show protocolMajor;

/// A daemon error (`code` is the snake_case ErrorCode).
class ProtocolException implements Exception {
  const ProtocolException(this.code, this.message, {this.retryAfterMs, this.matchedItems = const [], this.tooLarge});

  final ErrorCode code;
  final String message;

  /// `too_many_attempts`: how long until the server takes the next try.
  final int? retryAfterMs;

  /// `looks_like_secret` (B10): the names of the vault items the text holds
  /// the value of. Never a value.
  final List<String> matchedItems;

  /// `guideline_too_large` (B4): the combination of employee and project that
  /// would get too many characters of guidelines.
  final GuidelineTooLarge? tooLarge;

  /// The text holds a vault secret's value: nothing was stored (B10).
  bool get looksLikeSecret => code == ErrorCode.looksLikeSecret;

  bool get isForbidden => code == ErrorCode.forbidden;
  bool get isConflict => code == ErrorCode.versionConflict || code == ErrorCode.idempotencyConflict;

  /// The server's reply was too large to send (`frame_too_large`); asking again won't help.
  bool get isTooLarge => code == ErrorCode.frameTooLarge;

  /// Only a board member may do this (`board_member_required`): the app goes
  /// back to its sign-in screen.
  bool get needsBoardMember => code == ErrorCode.boardMemberRequired;

  /// The server does not have this yet: an older server (`unknown_method`), or
  /// one whose protocol 5 methods are still placeholders ("cannot do this yet",
  /// "… are not available on this server yet").
  bool get notOnServer =>
      code == ErrorCode.unknownMethod ||
      (code == ErrorCode.validation && RegExp(r'cannot do this yet|not (yet )?implemented|not available on this server yet', caseSensitive: false).hasMatch(message));

  /// What to show the user.
  String get shown => switch (this) {
        _ when needsBoardMember => boardMemberRequiredMessage,
        _ when notOnServer => notOnServerMessage,
        _ when isTooLarge && message.isEmpty => "The PC's reply is too large to send.",
        _ => message,
      };

  @override
  String toString() => shown;
}

/// `board_member_required`, with the step to take.
const boardMemberRequiredMessage = 'Only a board member can do this. Sign in on this phone first.';

/// [ProtocolException.notOnServer], with the step to take.
const notOnServerMessage = "The server can't do this yet. Update Aurelius on the server.";

/// The connection dropped before an answer arrived. The write may or may
/// not have been applied; resend with the same idempotency key to find out.
class DisconnectedException implements Exception {
  const DisconnectedException([this.message = 'The connection to the PC was lost.']);

  final String message;

  @override
  String toString() => message;
}

/// Streamed answer text: `text` continues the message at `offset`;
/// offset 0 carries the full text so far.
extension StreamAppendApply on StreamAppend {
  /// Apply to what the client has so far; `null` when it cannot continue
  /// (a missed piece), in which case the caller refetches.
  String? applyTo(String current) {
    if (offset == 0) return text;
    if (offset == current.length) return current + text;
    if (offset < current.length) return current.substring(0, offset) + text;
    return null;
  }
}

/// Chat read models (messages, conversations, employee presence).
extension EventKinds on Event {
  bool get isChat => switch (kind) {
        EventKind.conversationCreated ||
        EventKind.conversationDeleted ||
        EventKind.conversationSettingsChanged ||
        EventKind.conversationStarted ||
        EventKind.messageAssistantCommitted ||
        EventKind.messageDeleted ||
        EventKind.messageInterrupted ||
        EventKind.messageSentNow ||
        EventKind.messageUserCommitted ||
        EventKind.runCancelRequested ||
        EventKind.runCancelled ||
        EventKind.runCheckpointed ||
        EventKind.runCompleted ||
        EventKind.runFailed ||
        EventKind.runQueued ||
        EventKind.runSourceFetched ||
        EventKind.runStarted ||
        EventKind.runUsageReported ||
        EventKind.runWaiting =>
          true,
        _ => false,
      };

  bool get isRun => kind.wire.startsWith('run.');
  bool get isTask => kind.wire.startsWith('task.');
  bool get isIdea => kind.wire.startsWith('idea.');

  /// Project notes: only the Notes tab follows them (an autosave is one a second).
  bool get isNote => kind.wire.startsWith('note.');

  /// Memories and what was shared into them.
  bool get isMemory => kind.wire.startsWith('memory.') || kind.wire.startsWith('share.');

  /// Connections and the run defaults.
  bool get isRuntimeConfig => kind == EventKind.configChanged || kind == EventKind.connectionChanged;

  bool get isSchedule => kind == EventKind.recurringTaskChanged;

  /// Frequent bookkeeping that changes nothing a phone shows (streamed text
  /// arrives as appends; usage is fetched on demand).
  bool get isNoise => kind == EventKind.runCheckpointed || kind == EventKind.runUsageReported;
}
