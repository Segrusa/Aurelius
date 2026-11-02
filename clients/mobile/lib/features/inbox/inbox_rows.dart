import '../../data/views.dart';

/// A row of the personal Inbox (A3, freeze §4): one per ticket or channel, what
/// the server says about it. `reason` is text: an unknown one is a plain row.
extension PersonalRow on NotificationView {
  /// From the personal Inbox, not a result of a task or an announcement.
  bool get isPersonal => reason != null || ticketId != null || conversationId != null;

  /// The row's ◆. The server works it out when it is read, so it follows a handover.
  bool get waitsForMe => needsYou ?? false;

  /// How many more events the row stands for: the `+n`.
  int get more => mergedCount ?? 0;

  /// What the row is about: `#42 Login per board member`, `# general`.
  String get place => switch ((ticketNumber, subject.isEmpty)) {
        (final number?, false) => '#$number $subject',
        (final number?, true) => '#$number',
        (null, false) => conversationId != null ? '# $subject' : subject,
        (null, true) => '',
      };

  /// The head of the row, from its `reason`. The words that depend on who reads
  /// it follow [waitsForMe]; the rest names someone, so that is the server's sentence.
  String get headline => switch (reason) {
        'review' => waitsForMe ? 'Ready for your review' : 'Ready for review',
        'plan_review' => waitsForMe ? 'Plan ready for your review' : 'Plan ready for review',
        'failed' => 'The main task failed',
        'login' => 'Waiting for your Claude login',
        _ => title,
      };

  /// The row as one sentence for a screen reader (§9).
  String get spoken => [
        if (place.isNotEmpty) place,
        headline,
        if (waitsForMe) 'needs you',
        if (more > 0) '$more more ${more == 1 ? 'update' : 'updates'}',
        if (!read) 'unread',
      ].join(', ');
}

/// Where a row or a push of the personal Inbox opens: the ticket on the right
/// place, or the channel; null when it names neither.
String? inboxPlace({String? reason, String? ticketId, String? conversationId, String? messageId}) {
  if (ticketId != null) {
    return switch (reason) {
      'plan_review' => '/ticket/$ticketId/plan',
      'sent_back' => '/ticket/$ticketId?tab=discussion',
      'mention' when messageId != null => '/ticket/$ticketId?tab=discussion&message=$messageId',
      _ => '/ticket/$ticketId',
    };
  }
  if (conversationId != null) return messageId == null ? '/channel/$conversationId' : '/channel/$conversationId?message=$messageId';
  return null;
}

/// [inboxPlace] of a push's routing data: ids and the reason only. Its text
/// comes from the server, and the app never reads it to find the way.
String? pushPlace(Map<String, dynamic> data) =>
    inboxPlace(reason: data['reason'] as String?, ticketId: data['ticket_id'] as String?, conversationId: data['conversation_id'] as String?, messageId: data['message_id'] as String?);

/// Where the `+n` of a ticket's row opens (freeze §4: no list of events on the row).
String activityOf(String ticketId) => '/ticket/$ticketId?tab=activity';

/// The same row as read: all its fields stay.
NotificationView readRow(NotificationView n) => NotificationView(
      conversationId: n.conversationId,
      createdAtMs: n.createdAtMs,
      id: n.id,
      mergedCount: n.mergedCount,
      messageId: n.messageId,
      needsYou: n.needsYou,
      read: true,
      reason: n.reason,
      severity: n.severity,
      subject: n.subject,
      targetType: n.targetType,
      taskId: n.taskId,
      ticketId: n.ticketId,
      ticketNumber: n.ticketNumber,
      title: n.title,
    );
