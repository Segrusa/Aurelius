import 'dart:async';

import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/data/discussion.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/ticket_page.dart';
import 'package:aurelius_mobile/features/chat/chat_view.dart' show isSystemLine;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/phase2_wire.dart';
import 'support/tickets.dart';

// Phase 2 against the engine as it is (AUR-MU-02-M2): the server writes the
// sentence of every system line, puts a decision's feedback in the discussion
// itself, and counts unread and mentions per board member (reading marks read).
// The fixtures follow the server's JSON (support/phase2_wire.dart).

const _phase2 = LinkState(
  status: LinkStatus.online,
  capabilities: ['read', 'chat', 'task_control', 'approve'],
  features: [ticketDiscussionFeature],
  boardMembersExist: true,
  user: testUser,
);

const _lena = UserView(id: lenaId, username: 'lena', createdAtMs: 0);

TicketView _ticket(ColumnKind column) => TicketView.fromJson({
      ...ticketOf(column: column, ownerEmployeeId: 'forge', ownerName: 'Forge', rootTaskId: 'root').toJson(),
      'conversation_id': 'disc-1',
    });

Event _posted(String conversation, {int seq = 20}) => Event(aggregateId: conversation, atMs: 0, kind: EventKind.messageUserCommitted, seq: seq);

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  group('the wire shape', () {
    test('a discussion decodes: system lines carry their sentence, the feedback is the board member\'s own message', () {
      final messages = messagesResponse(discussionId, discussionLife()).messages;
      final lines = [for (final m in messages) if (m.system != null) (m.system!.kind, m.text)];
      expect(lines, [
        (SystemLineKind.moved, 'ada moved Backlog → Todo'),
        (SystemLineKind.plan, 'Forge handed in a plan · waiting for a board member'),
        (SystemLineKind.delegated, 'Forge delegated a task to Schema'),
        (SystemLineKind.review, 'Forge handed in the report · Review'),
        (SystemLineKind.done, 'ada put it on Done'),
        (SystemLineKind.summaryFailed, 'The summary for project memory failed'),
      ]);
      final feedback = messages.singleWhere((m) => m.text == 'Smaller steps');
      expect((feedback.isUser, authorOf(feedback), feedback.system), (true, 'ada', null));
      expect(messages.where((m) => m.system != null).every((m) => isSystemLine(m, inGroup: true) && m.author == 'Aurelius'), isTrue);
    });

    test('ticket_created names the source message and the number is read from the server\'s sentence (MU2-10 AC3)', () {
      final made = ticketsFromMessages(messagesResponse(orbitChannelId, channelWithTicket()).messages);
      expect(made.keys, ['01900000-0000-7000-8000-000000000201']);
      expect((made.values.single.ticketId, made.values.single.label), (madeTicketId, '#12'));
      expect(ticketNumberIn('team#3 created #12 from a message'), 12, reason: 'a name with a # of its own');
      expect(ticketNumberIn('ada created a ticket from a message'), isNull, reason: 'a ticket the server no longer finds');
    });
  });

  group('a ticket\'s discussion from the server', () {
    late MockRepository repo;
    late StreamController<Event> changes;
    late List<Map<String, dynamic>> wire;

    Future<void> open(WidgetTester tester, ColumnKind column, List<Map<String, dynamic>> messages, {Size size = const Size(390, 844)}) async {
      wire = messages;
      changes = StreamController<Event>.broadcast();
      addTearDown(changes.close);
      final ticket = _ticket(column);
      repo = boardRepository(detail: TicketDetail(asOfSeq: 1, ticket: ticket, tasks: const []));
      when(() => repo.messages(any())).thenAnswer((_) async => messagesResponse('disc-1', wire));
      when(() => repo.chatChanges).thenAnswer((_) => changes.stream);
      when(() => repo.appends).thenAnswer((_) => const Stream.empty());
      when(() => repo.loadChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.load()).thenAnswer((_) async => loadOf());
      when(() => repo.boardMembers()).thenAnswer((_) async => const [testUser, _lena]);
      await pumpBoardPage(tester, repo, TicketPage(ticketId: ticket.id, discussion: true), link: _phase2, size: size);
    }

    testWidgets('the server\'s sentences show as they are, the feedback once as ada\'s message', (tester) async {
      // Tall enough for the whole discussion: the list builds what is on screen.
      await open(tester, ColumnKind.done, discussionLife(), size: const Size(390, 3200));
      for (final sentence in ['ada moved Backlog → Todo', 'Forge delegated a task to Schema', 'ada put it on Done']) {
        expect(find.text(sentence), findsOneWidget, reason: sentence);
      }
      expect(find.text('Forge handed in a plan · waiting for a board member'), findsOneWidget, reason: 'the plan card');
      expect(find.text('Forge handed in the report · Review'), findsOneWidget, reason: 'the report card');
      expect(find.text('Smaller steps'), findsOneWidget);
      expect(find.text('ada · board member'), findsOneWidget, reason: 'the feedback is under its author');
      expect(find.text('The table is in.'), findsOneWidget, reason: 'an employee\'s copied answer');
    });

    testWidgets('Approve, accept edits with text in the input: one ticket.approve_plan with the feedback, no ticket.post; the server\'s copy shows once (MU2-05 AC3)', (tester) async {
      final upToPlan = discussionLife().take(3).toList();
      await open(tester, ColumnKind.planReview, upToPlan);
      await tester.enterText(find.byType(TextField).last, 'Smaller steps');
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('gate-approve-edits')));
      await settleBoard(tester);
      final write = writesOf(repo).single;
      expect(write['method'], 'ticket.approve_plan');
      expect(paramsOf(write), {'ticket_id': 'k1', 'decision': 'approve_edits', 'feedback': 'Smaller steps'});
      expect(find.text('Smaller steps'), findsNothing, reason: 'the composer is empty and the app shows no copy of its own');
      // The engine posts it as ada's message; the event brings it.
      wire = [...upToPlan, memberPost('01900000-0000-7000-8000-000000000104', 'Smaller steps', userId: adaId, username: 'ada')];
      changes.add(_posted('disc-1'));
      await settleBoard(tester);
      expect(find.text('Smaller steps'), findsOneWidget);
      verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
    });

    testWidgets('Send back with text: one ticket.send_back, nothing posted by the app (MU2-06)', (tester) async {
      await open(tester, ColumnKind.review, discussionLife().take(7).toList());
      await tester.enterText(find.byType(TextField).last, 'Add a test for the cost.');
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('bar-send-back')));
      await settleBoard(tester);
      expect(writesOf(repo).map((w) => w['method']), ['ticket.send_back']);
    });

    testWidgets('a failed summary: Try again sends ticket.retry_summary for the line\'s ticket (MU2-12 AC4)', (tester) async {
      await open(tester, ColumnKind.done, discussionLife());
      expect(find.text('⚠ The summary for project memory failed'), findsOneWidget);
      await tester.tap(find.byKey(const Key('system-line-action')));
      await settleBoard(tester);
      final write = writesOf(repo).single;
      expect(write['method'], 'ticket.retry_summary');
      expect(paramsOf(write), {'ticket_id': ticketId});
    });

    testWidgets('failed again after Try again: only the last line has Try again', (tester) async {
      await open(tester, ColumnKind.done, [...discussionLife(), summaryLine('01900000-0000-7000-8000-000000000110', saved: false)]);
      expect(find.text('⚠ The summary for project memory failed'), findsNWidgets(2));
      expect(find.byKey(const Key('system-line-action')), findsOneWidget);
    });

    testWidgets('saved after a failure: Try again is gone, the line says so (MU2-12 AC2)', (tester) async {
      await open(tester, ColumnKind.done, [...discussionLife(), summaryLine('01900000-0000-7000-8000-000000000110', saved: true)]);
      expect(find.text('Summary saved to project memory'), findsOneWidget);
      expect(find.byKey(const Key('system-line-action')), findsNothing);
    });
  });
}
