import 'dart:async';

import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/discussion.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/ticket_actions.dart';
import 'package:aurelius_mobile/features/board/ticket_page.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

// Phase 2, mobile (docs/product/fase-2): a ticket's discussion on the ticket
// page, its system lines, the decisions under its composer and the summary
// after Done. Neutral names only: the board members are ada (this phone) and lena.

const _phase2 = LinkState(
  status: LinkStatus.online,
  capabilities: ['read', 'chat', 'task_control', 'approve'],
  features: [ticketDiscussionFeature],
  boardMembersExist: true,
  user: testUser,
);

const _lena = UserView(id: 'u-2', username: 'lena', createdAtMs: 0);

/// A post in a discussion: a board member's ([by]) or an employee's ([author]).
MessageView postOf(String id, String text, {UserView? by, String? author}) => MessageView(
      activity: const [],
      author: author,
      authorUser: by,
      createdAtMs: 0,
      files: const [],
      id: id,
      ordinal: 0,
      queued: false,
      role: by != null ? MessageRole.user : MessageRole.assistant,
      runId: by != null ? null : 'run-$id',
      sources: const [],
      status: MessageStatus.complete,
      text: text,
      update: by != null ? null : 'group',
      uploads: const [],
    );

/// A system line of Aurelius (`MessageView.system`).
MessageView lineOf(String id, SystemLineKind kind, String text, {String? taskId, String? ticketId, String? messageId, String? conversationId, String? from}) => MessageView(
      activity: const [],
      author: 'Aurelius',
      createdAtMs: 0,
      files: const [],
      id: id,
      ordinal: 0,
      queued: false,
      role: MessageRole.assistant,
      sources: const [],
      status: MessageStatus.complete,
      system: SystemLine(kind: kind, taskId: taskId, ticketId: ticketId, messageId: messageId, conversationId: conversationId, from: from),
      text: text,
      uploads: const [],
    );

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  group('helpers', () {
    test('who a message without @ goes to (B3, P-21), and the hint that says so', () {
      expect(discussionAddressee(ticketOf(column: ColumnKind.doing, ownerName: 'Forge')), 'Forge');
      expect(discussionAddressee(ticketOf(column: ColumnKind.planReview, ownerName: 'Forge')), 'Forge');
      expect(discussionAddressee(ticketOf(column: ColumnKind.todo, ownerEmployeeId: 'forge', ownerName: 'Forge')), 'Forge');
      expect(discussionAddressee(ticketOf(column: ColumnKind.todo)), isNull, reason: 'Todo without an owner: nobody yet');
      expect(discussionAddressee(ticketOf(column: ColumnKind.review, ownerName: 'Forge')), 'Forge', reason: 'AUR-BUG-Q: the answer to a question in the report');
      for (final c in [ColumnKind.backlog, ColumnKind.done]) {
        expect(discussionAddressee(ticketOf(column: c, ownerName: 'Forge')), isNull, reason: '$c');
      }
      expect(discussionHint(ticketOf(column: ColumnKind.doing, ownerName: 'Forge')), 'Message Forge (owner) · @ to mention someone else');
      expect(discussionHint(ticketOf(column: ColumnKind.review, ownerName: 'Forge')), 'Message Forge (owner) · @ to mention someone else');
      expect(discussionHint(ticketOf(column: ColumnKind.backlog, ownerName: 'Forge')), 'No one is woken. @mention to ask someone.');
    });

    test('an archived ticket or a main task that ended: nobody is woken without @ (R1 c3/m1, as the server)', () {
      final archived = ticketOf(column: ColumnKind.doing, ownerName: 'Forge', archivedAtMs: 5);
      expect(discussionAddressee(archived), isNull);
      expect(discussionReadOnly(archived), 'Archived: the discussion can only be read.');
      expect(discussionReadOnly(ticketOf(column: ColumnKind.done)), contains('Done: the discussion can only be read.'));
      expect(discussionReadOnly(ticketOf(column: ColumnKind.doing)), isNull);
      for (final s in [TaskStatus.failed, TaskStatus.cancelled, TaskStatus.succeeded, TaskStatus.draft]) {
        expect(discussionAddressee(ticketOf(column: ColumnKind.doing, ownerName: 'Forge', rootTaskId: 'root', rootTaskStatus: s)), isNull, reason: '$s');
      }
      for (final s in [TaskStatus.queued, TaskStatus.running, TaskStatus.waiting, TaskStatus.review]) {
        expect(discussionAddressee(ticketOf(column: ColumnKind.doing, ownerName: 'Forge', rootTaskId: 'root', rootTaskStatus: s)), 'Forge', reason: '$s: the owner gets it in the main task');
      }
      expect(discussionHint(ticketOf(column: ColumnKind.doing, ownerName: 'Forge', rootTaskId: 'root', rootTaskStatus: TaskStatus.failed)),
          'The main task failed: no one is woken. Start makes a new one · @mention to ask someone.');
    });

    test('@names: in order and once, not in an e-mail address; the ones that are nobody (MU2-04 AC5)', () {
      expect(mentionedNames('@Forge and @forge, ask @Schema. Mail me@host.io'), ['Forge', 'Schema']);
      expect(mentionedNames('Done @Forge.'), ['Forge'], reason: 'the dot ends the sentence');
      expect(unknownMentions('@Forge @Nobody @lena @Ana', ['Forge', 'lena', 'Ana Maria']), ['Nobody'], reason: 'a name with a space is written with its first word');
      expect(mentionsName('Thanks @ada', 'ada'), isTrue);
      expect(mentionsName('Thanks @adam', 'ada'), isFalse);
      expect(mentionsName('Thanks @ada', null), isFalse);
    });

    test('unknown_method of ticket.post and ticket.retry_summary says the server cannot yet', () {
      const unknown = ProtocolException(ErrorCode.unknownMethod, 'unknown method');
      expect(discussionPostFailure(unknown), contains('cannot take messages in a ticket discussion yet'));
      expect(summaryRetryFailure(unknown), contains('cannot try the summary again yet'));
      expect(discussionPostFailure(const ProtocolException(ErrorCode.forbidden, 'Done: read only')), 'Done: read only');
    });

    test('a ticket from a message: first line as title, the message quoted with where it came from (MU2-10 AC1)', () {
      final m = messageOf('m1', MessageRole.user, '## Add an audit export\nWith the date of each change.');
      final draft = draftFromMessage(m, const TicketSource(from: 'Team chat · #atlas', projectId: 'p1'), author: 'lena', at: DateTime(2026, 10, 30, 14, 5));
      expect(draft.title, 'Add an audit export');
      expect(draft.description, '> ## Add an audit export\n> With the date of each change.\n\n*From: Team chat · #atlas · lena · Oct 30 14:05*');
      expect(draft.projectId, 'p1');
      expect(draft.fromMessage, 'm1');
      final long = draftFromMessage(messageOf('m2', MessageRole.user, 'x' * 300), const TicketSource(from: 'Team chat · #general'));
      expect(long.title.length, 160);
      expect(long.projectId, isNull, reason: 'in #general the form asks for a project');
    });

    test('the tickets made from messages, from the ticket_created lines (MU2-10 AC3)', () {
      final made = ticketsFromMessages([
        postOf('m1', 'An idea', by: _lena),
        lineOf('s1', SystemLineKind.ticketCreated, 'lena created #52 from a message', ticketId: 'k52', messageId: 'm1'),
        lineOf('s2', SystemLineKind.moved, 'Forge moved #3 Doing → Review'),
      ]);
      expect(made.keys, ['m1']);
      expect(made['m1']!.ticketId, 'k52');
      expect(made['m1']!.label, '#52');
    });

    test('a system line reads in full for a screen reader (§5)', () {
      expect(systemLineLabel(lineOf('s', SystemLineKind.moved, 'Forge moved #42 from Doing to Review')), 'System: Forge moved #42 from Doing to Review');
    });
  });

  group('the session of a ticket (ticket #71: its conversation is the main view)', () {
    late MockRepository repo;

    Future<void> open(WidgetTester tester, TicketView ticket, List<MessageView> messages, {TaskView? root, LinkState link = _phase2}) async {
      final detail = TicketDetail(asOfSeq: 1, ticket: ticket, rootTask: root, tasks: [?root]);
      repo = boardRepository(detail: detail, messages: snapshotOf('disc-1', messages: messages));
      when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.appends).thenAnswer((_) => const Stream.empty());
      when(() => repo.loadChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.load()).thenAnswer((_) async => loadOf());
      when(() => repo.boardMembers()).thenAnswer((_) async => const [testUser, _lena]);
      await pumpBoardPage(tester, repo, TicketPage(ticketId: ticket.id), link: link);
    }

    TicketView discussed({ColumnKind column = ColumnKind.doing, int? archivedAtMs, TaskStatus? rootTaskStatus}) => TicketView.fromJson({
          ...ticketOf(column: column, ownerEmployeeId: 'forge', ownerName: 'Forge', rootTaskId: 'root', archivedAtMs: archivedAtMs, rootTaskStatus: rootTaskStatus).toJson(),
          'conversation_id': 'disc-1',
        });

    Future<void> send(WidgetTester tester, String text) async {
      await tester.enterText(find.byType(TextField).last, text);
      await tester.pump();
      await tester.tap(find.byKey(const Key('composer-send')));
      await settleBoard(tester);
    }

    testWidgets('board members and employees post there, with system lines; a delegation opens its task (MU2-01 AC2, MU2-02 AC1/AC2, MU2-03)', (tester) async {
      await open(tester, discussed(), [
        postOf('m1', 'Please keep the hash parameters configurable.', by: _lena),
        lineOf('s1', SystemLineKind.delegated, 'Orion delegated "Users table" to Schema', taskId: 'sub', from: 'Orion'),
        postOf('m2', 'Done. @ada the cost is set via config.', author: 'Forge'),
        lineOf('s2', SystemLineKind.moved, 'Forge moved #1 Doing → Review'),
      ]);
      expect(find.text('lena · board member'), findsOneWidget, reason: 'the author of a human post (A-01)');
      expect(find.textContaining('Forge — Backend Developer'), findsOneWidget, reason: 'an employee with its role');
      expect(find.textContaining('@ you'), findsOneWidget, reason: 'a post naming this board member is marked (B9)');
      expect(find.text('Forge moved #1 Doing → Review'), findsOneWidget);
      expect(find.bySemanticsLabel('System: Forge moved #1 Doing → Review'), findsOneWidget);
      await tester.tap(find.text('Orion delegated "Users table" to Schema'));
      await settleBoard(tester);
      expect(find.text('task sub'), findsOneWidget);
    });

    testWidgets('MU3-11 / ui §5: the server\'s Paused and Resumed lines (SystemLineKind.limit) show as quiet system lines', (tester) async {
      await open(tester, discussed(), [
        lineOf('s1', SystemLineKind.limit, "Paused: robin's Claude limit is reached until 16:00."),
        postOf('m1', 'Ok, we wait.', by: _lena),
        lineOf('s2', SystemLineKind.limit, 'Resumed.'),
      ]);
      expect(find.text("Paused: robin's Claude limit is reached until 16:00."), findsOneWidget);
      expect(find.bySemanticsLabel("System: Paused: robin's Claude limit is reached until 16:00."), findsOneWidget);
      expect(find.text('Resumed.'), findsOneWidget);
      expect(find.textContaining('was not woken'), findsNothing, reason: 'the server\'s own text, not a made-up one');
    });

    testWidgets('a message goes out with ticket.post; without @ the placeholder names the owner (BR1, MU2-02 AC1/AC3)', (tester) async {
      await open(tester, discussed(), const []);
      expect(find.text('Discuss the ticket here'), findsOneWidget);
      expect(find.text('Message Forge (owner)…'), findsOneWidget);
      expect(find.text('@Name asks that employee; without a name, Forge (owner) answers.'), findsWidgets);
      await send(tester, 'Use the config file.');
      final post = writesOf(repo).single;
      expect(post['method'], 'ticket.post');
      expect(paramsOf(post), {'files': <String>[], 'images': <String>[], 'text': 'Use the config file.', 'ticket_id': 'k1'});
    });

    testWidgets('a server without the engine (unknown_method): the message stays with Try again and the app says why', (tester) async {
      await open(tester, discussed(), const []);
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenThrow(const ProtocolException(ErrorCode.unknownMethod, 'unknown method: ticket.post'));
      await send(tester, 'Hello?');
      expect(find.textContaining('cannot take messages in a ticket discussion yet'), findsOneWidget);
      expect(find.text('Not sent'), findsOneWidget);
      expect(find.text('Try again'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets('@ offers employees and the other board members; a name that is nobody says so (MU2-04 AC5, B9)', (tester) async {
      await open(tester, discussed(), const []);
      await tester.enterText(find.byType(TextField).last, 'Ask @l');
      await tester.pump();
      expect(find.text('@lena — board member'), findsOneWidget);
      await tester.enterText(find.byType(TextField).last, 'Ask @a');
      await tester.pump();
      expect(find.byKey(const Key('mentions')), findsNothing, reason: 'this board member is not offered to itself');
      await send(tester, '@Nobody can you look?');
      expect(find.text('@Nobody is nobody here: no one is woken for it.'), findsOneWidget);
      expect(writesOf(repo).single['method'], 'ticket.post', reason: 'the post still goes out');
    });

    testWidgets('on Done the discussion can only be read (MU2-01 AC3, B7)', (tester) async {
      await open(tester, discussed(column: ColumnKind.done), [postOf('m1', 'Shipped.', author: 'Forge')]);
      expect(find.byKey(const Key('chat-read-only')), findsOneWidget);
      expect(find.byKey(const Key('composer-send')), findsNothing);
      expect(find.text('Shipped.'), findsOneWidget);
    });

    testWidgets('an archived ticket: the discussion can only be read, also in Review (R1 c3, B7)', (tester) async {
      await open(tester, discussed(column: ColumnKind.review, archivedAtMs: 5), [postOf('m1', 'Parked.', author: 'Forge')]);
      expect(find.byKey(const Key('chat-read-only')), findsOneWidget);
      expect(find.text('#1 is archived · read-only.'), findsOneWidget);
      expect(find.byKey(const Key('composer-send')), findsNothing);
      expect(find.byKey(const ValueKey('bar-send-back')), findsNothing, reason: 'no gate on an archived ticket (BR5)');
      expect(find.byKey(const ValueKey('chip-mode')), findsNothing);
      expect(find.text('Parked.'), findsOneWidget);
    });

    testWidgets('a failed main task: the hint says nobody is woken; the server\'s Nobody line after the post is a system line, and no one is named as reached (R1 m1/m3, F3)', (tester) async {
      await open(tester, discussed(rootTaskStatus: TaskStatus.failed), const [], root: taskOf(id: 'root', status: TaskStatus.failed));
      expect(find.text('Message #1…'), findsOneWidget);
      expect(find.text('@Name asks that employee; without a name, no one is woken.'), findsWidgets);
      expect(find.text('Message Forge (owner)…'), findsNothing);
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((_) async => receiptOf(const CommandResultTicketMessagePosted(conversationId: 'disc-1', messageId: 'm1', runIds: [], ticketId: 'k1', to: [])));
      const nobody = 'Nobody was woken: the main task of #1 failed. Start makes a new one, or name a colleague with @Name.';
      when(() => repo.messages(any())).thenAnswer((_) async => snapshotOf('disc-1', messages: [
            postOf('m1', 'Any news?', by: testUser),
            lineOf('s1', SystemLineKind.limit, nobody),
          ]));
      await send(tester, 'Any news?');
      expect(writesOf(repo).single['method'], 'ticket.post');
      expect(find.text(nobody), findsOneWidget);
      expect(find.bySemanticsLabel('System: $nobody'), findsOneWidget, reason: 'meta.system: a quiet line, not a message of "Aurelius"');
      expect(find.textContaining('Aurelius'), findsNothing);
      expect(find.byType(SnackBar), findsNothing, reason: 'the app names nobody as reached');
      expect(tester.takeException(), isNull);
    });

    testWidgets('Plan review: the gate above the input; Request changes takes the input\'s text (MU2-05 AC1/AC3, TS-04)', (tester) async {
      await open(tester, discussed(column: ColumnKind.planReview), [lineOf('s1', SystemLineKind.plan, 'Forge handed in a plan.', taskId: 'root', from: 'Forge')]);
      expect(find.text('◆ PLAN · FORGE'), findsOneWidget);
      expect(find.text('Plan review · your decision'), findsOneWidget);
      final revise = find.byKey(const ValueKey('gate-revise'));
      await tester.tap(revise);
      await settleBoard(tester);
      expect(find.text('Write what needs to change below, then Request changes.'), findsOneWidget, reason: 'Request changes needs words (B2)');
      verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
      await tester.enterText(find.byType(TextField).last, 'Split the migration in two.');
      await tester.pump();
      await tester.tap(revise);
      await settleBoard(tester);
      final write = writesOf(repo).single;
      expect(write['method'], 'ticket.revise_plan');
      expect(paramsOf(write), {'ticket_id': 'k1', 'feedback': 'Split the migration in two.'});
      expect(tester.widget<TextField>(find.byType(TextField).last).controller!.text, isEmpty, reason: 'sent: the composer is empty again');
      await tester.tap(find.text('Review plan'));
      await settleBoard(tester);
      expect(find.text('plan k1'), findsOneWidget);
    });

    testWidgets('Plan review: Approve approves from the gate (MU2-05 AC2)', (tester) async {
      await open(tester, discussed(column: ColumnKind.planReview), [lineOf('s1', SystemLineKind.plan, 'Forge handed in a plan.', taskId: 'root', from: 'Forge')]);
      await tester.tap(find.byKey(const ValueKey('gate-approve')));
      await settleBoard(tester);
      final write = writesOf(repo).single;
      expect(write['method'], 'ticket.approve_plan');
      expect(paramsOf(write), {'ticket_id': 'k1', 'decision': 'approve'});
    });

    testWidgets('Review: Send back needs words, then ticket.send_back; Done asks first (MU2-06 AC1/AC2)', (tester) async {
      await open(tester, discussed(column: ColumnKind.review), [lineOf('s1', SystemLineKind.review, 'Forge reported.', taskId: 'root', from: 'Forge')]);
      expect(find.text('In review · your decision'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('bar-done')));
      await settleBoard(tester);
      expect(find.text('Mark #1 as Done?'), findsOneWidget, reason: 'the confirmation of Phase 1 (B6 unchanged)');
      await tester.tap(find.text('Cancel'));
      await settleBoard(tester);
      await tester.tap(find.byKey(const ValueKey('bar-send-back')));
      await settleBoard(tester);
      expect(find.text('Write what needs to change below, then Send back.'), findsOneWidget);
      verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
      await tester.enterText(find.byType(TextField).last, 'Add a test for the cost.');
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('bar-send-back')));
      await settleBoard(tester);
      final write = writesOf(repo).single;
      expect(write['method'], 'ticket.send_back');
      expect(paramsOf(write), {'ticket_id': 'k1', 'feedback': 'Add a test for the cost.'});
    });

    testWidgets('Testing: a word goes to the owner as in Review, without Send back or Done (ticket #5)', (tester) async {
      await open(tester, discussed(column: ColumnKind.testing, rootTaskStatus: TaskStatus.review), const []);
      expect(find.text('Message Forge (owner)…'), findsOneWidget);
      expect(find.byKey(const ValueKey('bar-send-back')), findsNothing);
      expect(find.byKey(const ValueKey('bar-done')), findsNothing);
      await send(tester, 'Which suite fails?');
      expect(writesOf(repo).single['method'], 'ticket.post');
    });

    testWidgets('Review: the report card opens the report (ui-ontwerp §2 "Report ›")', (tester) async {
      await open(tester, discussed(column: ColumnKind.review), [lineOf('s1', SystemLineKind.review, 'Forge reported.', taskId: 'root', from: 'Forge')]);
      await tester.tap(find.text('Report ›'));
      await settleBoard(tester);
      expect(find.text('report k1'), findsOneWidget);
    });

    testWidgets('the gate sits right above the input, the run chips and the hint under it (ui-ontwerp §3.1)', (tester) async {
      await open(tester, discussed(column: ColumnKind.review), const []);
      final gate = tester.getRect(find.byKey(const ValueKey('gate-review')));
      final input = tester.getRect(find.byType(TextField).last);
      final chips = tester.getRect(find.byKey(const ValueKey('chip-mode')));
      expect(gate.bottom <= input.top, isTrue, reason: 'the gate is above the input');
      expect(input.top - gate.bottom < 40, isTrue, reason: 'directly above it');
      expect(chips.top >= input.bottom, isTrue, reason: 'Mode and Model are under the input');
      expect(find.text('Send #1 back to Doing'), findsNothing, reason: 'no separate feedback sheet (MU2-05 AC4)');
    });

    testWidgets('a failed summary is visible with Try again: ticket.retry_summary (MU2-12 AC4)', (tester) async {
      await open(tester, discussed(column: ColumnKind.done), [lineOf('s1', SystemLineKind.summaryFailed, 'The summary failed: the quota is used up.')]);
      expect(find.text('⚠ The summary failed: the quota is used up.'), findsOneWidget);
      await tester.tap(find.byKey(const Key('system-line-action')));
      await settleBoard(tester);
      final write = writesOf(repo).single;
      expect(write['method'], 'ticket.retry_summary');
      expect(paramsOf(write), {'ticket_id': 'k1'});
      expect(find.text('Trying the summary again.'), findsOneWidget);
    });

    testWidgets('Try again on a server without the engine (unknown_method) says so (MU2-12 AC4)', (tester) async {
      await open(tester, discussed(column: ColumnKind.done), [lineOf('s1', SystemLineKind.summaryFailed, 'The summary failed.')]);
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenThrow(const ProtocolException(ErrorCode.unknownMethod, 'unknown method: ticket.retry_summary'));
      await tester.tap(find.byKey(const Key('system-line-action')));
      await settleBoard(tester);
      expect(find.textContaining('cannot try the summary again yet'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets('once the summary is in, Try again is gone; the summary is a line (MU2-12 AC2)', (tester) async {
      await open(tester, discussed(column: ColumnKind.done), [
        lineOf('s1', SystemLineKind.summaryFailed, 'The summary failed.'),
        lineOf('s2', SystemLineKind.summary, 'Ticket #1 “Write the release notes” is done: the notes are in docs/.'),
      ]);
      expect(find.byKey(const Key('system-line-action')), findsNothing);
      expect(find.text('Ticket #1 “Write the release notes” is done: the notes are in docs/.'), findsOneWidget);
    });

    testWidgets('a new message shows in the session without a reload', (tester) async {
      // Its own conversation: what this phone saw is kept per conversation while the app runs.
      final ticket = TicketView.fromJson({...discussed().toJson(), 'conversation_id': 'disc-dot'});
      final online = StreamController<void>.broadcast();
      addTearDown(online.close);
      var messages = [postOf('m1', 'First.', author: 'Forge')];
      repo = boardRepository(detail: TicketDetail(asOfSeq: 1, ticket: ticket, tasks: const []));
      when(() => repo.messages(any())).thenAnswer((_) async => snapshotOf('disc-dot', messages: messages));
      when(() => repo.online).thenAnswer((_) => online.stream);
      when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.appends).thenAnswer((_) => const Stream.empty());
      when(() => repo.loadChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.load()).thenAnswer((_) async => loadOf());
      await pumpBoardPage(tester, repo, TicketPage(ticketId: ticket.id), link: _phase2);
      expect(find.text('First.'), findsOneWidget);
      messages = [...messages, postOf('m2', 'Second.', author: 'Forge')];
      online.add(null);
      await settleBoard(tester);
      expect(find.text('Second.'), findsOneWidget);
    });

    testWidgets('the plan page sends Pas aan to the discussion, without a field of its own (MU2-05 AC4)', (tester) async {
      final ticket = discussed(column: ColumnKind.planReview);
      repo = boardRepository(detail: TicketDetail(asOfSeq: 1, ticket: ticket, tasks: const []));
      await pumpBoardPage(tester, repo, TicketPlanPage(ticketId: ticket.id), link: _phase2);
      expect(find.byKey(const ValueKey('ticket-plan-feedback')), findsNothing);
      expect(find.byKey(const ValueKey('ticket-plan-discuss')), findsOneWidget);
      expect(find.text('Request changes in the discussion'), findsOneWidget, reason: 'a new label is English (D-01, QA1 m5)');
      expect(find.textContaining('in de discussie'), findsNothing);
      expect(find.byKey(const ValueKey('ticket-plan-approve')), findsOneWidget, reason: 'approving needs no words');
    });

    testWidgets('dropping a Review card on Doing opens the discussion, not a feedback sheet (MU2-06 AC1)', (tester) async {
      final ticket = discussed(column: ColumnKind.review);
      repo = boardRepository(detail: TicketDetail(asOfSeq: 1, ticket: ticket, tasks: const []));
      final router = await pumpBoardPage(
        tester,
        repo,
        Builder(builder: (context) => Scaffold(body: TextButton(onPressed: () => moveTicket(context, ticket, columnOf(ColumnKind.doing)), child: const Text('drop on Doing')))),
        link: _phase2,
      );
      await tester.tap(find.text('drop on Doing'));
      await settleBoard(tester);
      expect(router.state.uri.toString(), '/ticket/k1');
      expect(find.text('Send #1 back to Doing'), findsNothing);
      verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
    });

    testWidgets('an older server (no ticket conversation): head, gate and properties, and it says so (ticket #71)', (tester) async {
      final ticket = discussed(column: ColumnKind.review);
      repo = boardRepository(detail: TicketDetail(asOfSeq: 1, ticket: ticket, tasks: const []));
      await pumpBoardPage(tester, repo, TicketPage(ticketId: ticket.id));
      expect(find.text('This server has no ticket conversation.'), findsOneWidget);
      expect(find.byKey(const Key('composer-send')), findsNothing);
      expect(find.byKey(const ValueKey('chip-mode')), findsNothing, reason: 'no input, no run chips');
      expect(find.byKey(const ValueKey('gate-review')), findsOneWidget);
      expect(find.byTooltip('Ticket actions'), findsOneWidget, reason: 'Properties are in its menu');
      await tester.tap(find.byKey(const ValueKey('bar-send-back')));
      await settleBoard(tester);
      expect(find.text('Send #1 back to Doing'), findsOneWidget, reason: 'the feedback sheet of Phase 1');
    });
  });
}
