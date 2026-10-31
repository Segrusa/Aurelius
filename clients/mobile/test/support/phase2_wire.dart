import 'package:aurelius_mobile/data/protocol/protocol.dart';
import 'package:aurelius_mobile/data/views.dart';

// Phase 2 as the server sends it (protocol 4, frozen in AUR-MU-02-F1): the JSON
// of `conversation.get`, `group_chat.get {channel}` and `channel.list`, decoded
// by the generated protocol, the way crates/storage/src/conversations.rs
// (list_messages) and crates/application/src/conversation/discussion.rs
// (post_system, decision_feedback) write it. A system line is an update of
// Aurelius (role `assistant`, update `status`) with its sentence as the text
// (`SystemLine::sentence`, with #N) and `system`; a decision's feedback is the
// board member's own message (role `user`, `author_user`). Neutral names only.

const adaId = '01900000-0000-7000-8000-0000000000a1';
const lenaId = '01900000-0000-7000-8000-0000000000a2';
const discussionId = '01900000-0000-7000-8000-0000000000d1';
const generalId = '01900000-0000-7000-8000-0000000000c1';
const orbitChannelId = '01900000-0000-7000-8000-0000000000c2';
const orbitProjectId = '01900000-0000-7000-8000-0000000000b1';
const ticketId = '01900000-0000-7000-8000-0000000000e1';
const madeTicketId = '01900000-0000-7000-8000-0000000000e2';
const rootTaskId = '01900000-0000-7000-8000-0000000000f1';

Map<String, dynamic> userJson(String id, String username) => {'id': id, 'username': username, 'created_at_ms': 1700000000000, 'created_by_user': null};

int _ordinal = 0;

Map<String, dynamic> _message(String id, {required String role, required String text, String? author, String? update, Map<String, dynamic>? authorUser, Map<String, dynamic>? system, String? runId}) => {
      'id': id,
      'ordinal': ++_ordinal,
      'role': role,
      'status': 'complete',
      'text': text,
      'created_at_ms': 1700000000000 + _ordinal * 1000,
      'run_id': runId,
      'reply_to': null,
      'produced_by': null,
      'author': author,
      'sources': <dynamic>[],
      'activity': <dynamic>[],
      'update': update,
      'approval_id': null,
      'files': <dynamic>[],
      'uploads': <dynamic>[],
      'queued': false,
      'author_user': ?authorUser,
      'system': ?system,
    };

/// A board member's message in a discussion or channel (also a decision's feedback).
Map<String, dynamic> memberPost(String id, String text, {String userId = lenaId, String username = 'lena'}) =>
    _message(id, role: 'user', text: text, authorUser: userJson(userId, username));

/// An employee's answer copied to a discussion or channel (`mirror_answer`: its name as author).
Map<String, dynamic> employeePost(String id, String text, String employee) => _message(id, role: 'assistant', text: text, author: employee, update: 'group', runId: '01900000-0000-7000-8000-00000000ff${id.hashCode.abs() % 90 + 10}');

/// A system line as `post_system` writes it: Aurelius, kind `status`, the sentence as text.
Map<String, dynamic> systemPost(String id, String sentence, Map<String, dynamic> line) =>
    _message(id, role: 'assistant', text: sentence, author: 'Aurelius', update: 'status', system: line);

MessagesSnapshot messagesResponse(String conversationId, List<Map<String, dynamic>> messages) {
  final message = ServerMessage.fromJson({
    'type': 'response',
    'request_id': 7,
    'ok': {
      'type': 'messages',
      'value': {
        'as_of_seq': 90,
        'conversation_id': conversationId,
        'messages': messages,
        'attachments': <dynamic>[],
        'agent_mode': 'ask',
        'browser_allowed': false,
        'screen_allowed': false,
        'run': <String, dynamic>{},
        'shares': <dynamic>[],
      },
    },
  }) as ServerMessageResponse;
  return (message.ok! as ResponseBodyMessages).value;
}

/// `channel.list` as the daemon answers it (crates/daemon/tests/channels.rs):
/// General first, then the projects; unread and mentions per board member.
List<ChannelView> channelsResponse(List<Map<String, dynamic>> channels) {
  final message = ServerMessage.fromJson({
    'type': 'response',
    'request_id': 8,
    'ok': {
      'type': 'channels',
      'value': {'as_of_seq': 91, 'channels': channels},
    },
  }) as ServerMessageResponse;
  return (message.ok! as ResponseBodyChannels).value.channels;
}

Map<String, dynamic> channelJson(String id, String name, {String? projectId, int unread = 0, int mentions = 0, bool archived = false}) => {
      'conversation_id': id,
      'project_id': ?projectId,
      'name': name,
      'unread': unread,
      'mentions': mentions,
      'archived': archived,
      'last_activity_ms': 1700000005000,
    };

/// A ticket's discussion through its life: posts, the engine's lines, the
/// feedback of Pas aan as ada's own message, Done and a failed summary.
List<Map<String, dynamic>> discussionLife() => [
      memberPost('01900000-0000-7000-8000-000000000101', 'Please keep the hash parameters configurable.'),
      systemPost('01900000-0000-7000-8000-000000000102', 'ada moved Backlog → Todo', {'kind': 'moved', 'by': 'ada', 'from': 'Backlog', 'to': 'Todo', 'ticket_id': ticketId}),
      systemPost('01900000-0000-7000-8000-000000000103', 'Forge handed in a plan · waiting for a board member', {'kind': 'plan', 'task_id': rootTaskId, 'from': 'Forge'}),
      memberPost('01900000-0000-7000-8000-000000000104', 'Smaller steps', userId: adaId, username: 'ada'),
      systemPost('01900000-0000-7000-8000-000000000105', 'Forge delegated a task to Schema', {'kind': 'delegated', 'task_id': '01900000-0000-7000-8000-0000000000f2', 'from': 'Forge', 'to': 'Schema'}),
      employeePost('01900000-0000-7000-8000-000000000106', 'The table is in.', 'Schema'),
      systemPost('01900000-0000-7000-8000-000000000107', 'Forge handed in the report · Review', {'kind': 'review', 'task_id': rootTaskId, 'from': 'Forge'}),
      systemPost('01900000-0000-7000-8000-000000000108', 'ada put it on Done', {'kind': 'done', 'by': 'ada', 'ticket_id': ticketId}),
      systemPost('01900000-0000-7000-8000-000000000109', 'The summary for project memory failed', {'kind': 'summary_failed', 'ticket_id': ticketId}),
    ];

/// A summary line after [discussionLife]: saved, or failed once more.
Map<String, dynamic> summaryLine(String id, {required bool saved}) => saved
    ? systemPost(id, 'Summary saved to project memory', {'kind': 'summary', 'ticket_id': ticketId})
    : systemPost(id, 'The summary for project memory failed', {'kind': 'summary_failed', 'ticket_id': ticketId});

/// The Orbit channel where lena's message became ticket #12 (`from_message`):
/// the server adds `ticket_created` with the source message's id.
List<Map<String, dynamic>> channelWithTicket() => [
      memberPost('01900000-0000-7000-8000-000000000201', 'The export button does nothing on Safari.'),
      employeePost('01900000-0000-7000-8000-000000000202', 'I can look at it.', 'Bo'),
      systemPost('01900000-0000-7000-8000-000000000203', 'ada created #12 from a message', {
        'kind': 'ticket_created',
        'by': 'ada',
        'ticket_id': madeTicketId,
        'conversation_id': orbitChannelId,
        'message_id': '01900000-0000-7000-8000-000000000201',
      }),
    ];
