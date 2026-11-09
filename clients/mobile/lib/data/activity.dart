import 'protocol/protocol.dart';

/// The server has `activity.overview` (ticket #50).
const activityOverviewFeature = 'activity_overview';

/// What stands for another board member's chat: the server sends nothing else of it.
const privateChatLabel = 'Privégesprek';

/// Work held back only by a free slot, of its employee or of this machine.
bool waitsForSlot(ActivityWaiting w) => w.waitsFor == 'slot' || w.waitsFor == 'machine';

/// What a row is about.
String workLabel(ActivityWork work) => work.private == true ? privateChatLabel : work.label;

/// Why something waits, in words. [who]: the employee it waits with.
String waitLabel(String? waitsFor, String? who) => switch (waitsFor) {
      'slot' => who == null ? 'Waiting for a free slot' : 'Waiting for a free slot: $who is busy',
      'machine' => 'Waiting for a free slot',
      'paused' => who == null ? 'Paused' : 'Paused: $who',
      'question' || 'board_member' => 'Waiting for a board member',
      'approval' => 'Waiting for an approval',
      'colleague' => 'Waiting for a colleague',
      'limit' => 'Waiting for the usage limit',
      'login' => 'Waiting for a Claude login',
      'later' => 'Scheduled for later',
      'handoff' => 'Waiting for a handoff',
      _ => 'Waiting',
    };

/// One line on a card or the ticket page.
String ticketActivityLine(TicketActivity a) => switch (a.state) {
      'running' => a.employeeName == null ? 'Running' : 'Running: ${a.employeeName}',
      'paused' => waitLabel('paused', a.employeeName),
      _ => waitLabel(a.waitsFor, a.employeeName),
    };

/// "since 5 min", "since 2 h".
String sinceText(int sinceMs, int nowMs) {
  final minutes = ((nowMs - sinceMs) / 60000).floor();
  if (minutes < 1) return 'just now';
  if (minutes < 60) return 'since $minutes min';
  return 'since ${(minutes / 60).floor()} h';
}
