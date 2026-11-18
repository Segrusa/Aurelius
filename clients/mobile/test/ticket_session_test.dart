import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/channels.dart';
import 'package:aurelius_mobile/data/discussion.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/ticket_page.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

// A ticket as a session (ticket #71, AUR-T71-M2): the conversation is the main
// view, the head says #N, title, column and who, the gate of the column sits
// right above the input and the ticket's data is in a sheet. One test per
// column the brief names (Doing, Review, Plan review, Done), and the edges.

const _session = LinkState(
  status: LinkStatus.online,
  capabilities: ['read', 'chat', 'task_control', 'approve'],
  features: [ticketDiscussionFeature, teamChannelsFeature, ticketChannelsFeature, 'ticket_responsible'],
  boardMembersExist: true,
  user: testUser,
  claudeLogin: testLogin,
);

MessageView _post(String id, String text, {String? author, UserView? by}) => MessageView(
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

MessageView _line(String id, SystemLineKind kind, String text, {String? taskId}) => MessageView(
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
      system: SystemLine(kind: kind, taskId: taskId, from: 'Forge'),
      text: text,
      uploads: const [],
    );

TicketView _ticket({int number = 42, ColumnKind column = ColumnKind.doing, int? archivedAtMs, TaskStatus? rootTaskStatus, TaskRun run = const TaskRun(), UserView? responsible}) => TicketView.fromJson({
      ...ticketOf(number: number, title: 'Hash passwords with argon2', column: column, ownerEmployeeId: 'forge', ownerName: 'Forge', rootTaskId: 'root', archivedAtMs: archivedAtMs, rootTaskStatus: rootTaskStatus, run: run, responsible: responsible).toJson(),
      'conversation_id': 'disc-1',
    });

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  late MockRepository repo;

  /// The session of [ticket] with [messages] in its conversation and [rootMessages] in its main task's.
  Future<void> open(WidgetTester tester, TicketView ticket, {List<MessageView> messages = const [], TaskView? root, List<MessageView>? rootMessages, LinkState link = _session, Widget? page, List<TimelineCard> activity = const []}) async {
    final main = root ?? taskOf(id: 'root', status: TaskStatus.running, employeeId: 'forge', employeeName: 'Forge', conversationId: 'c-root');
    repo = boardRepository(detail: TicketDetail(asOfSeq: 1, ticket: ticket, rootTask: main, tasks: [main]), link: link, activity: activity);
    when(() => repo.messages(any())).thenAnswer((i) async => i.positionalArguments.first == 'c-root' ? snapshotOf('c-root', messages: rootMessages ?? const []) : snapshotOf('disc-1', messages: messages));
    when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.appends).thenAnswer((_) => const Stream.empty());
    when(() => repo.loadChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.load()).thenAnswer((_) async => loadOf());
    when(() => repo.boardMembers()).thenAnswer((_) async => const [testUser, bo]);
    await pumpBoardPage(tester, repo, page ?? TicketPage(ticketId: ticket.id), link: link);
  }

  Future<void> tapKey(WidgetTester tester, String key) async {
    await tester.ensureVisible(find.byKey(ValueKey(key)));
    await tester.pump();
    await tester.tap(find.byKey(ValueKey(key)));
    await settleBoard(tester);
  }

  group('Doing', () {
    testWidgets('one screen: the head with #N, title, column and who, the conversation, the BR1 placeholder, no gate while it runs', (tester) async {
      await open(tester, _ticket(responsible: bo), messages: [_post('m1', 'I am on the migration.', author: 'Forge')]);
      expect(find.text('#42 · Atlas'), findsOneWidget);
      expect(find.byKey(const ValueKey('ticket-title')), findsOneWidget);
      expect(find.text('Hash passwords with argon2'), findsOneWidget);
      expect(find.descendant(of: find.byKey(const ValueKey('ticket-column')), matching: find.text('Doing')), findsOneWidget);
      expect(find.text('owner Forge · responsible bo · Atlas'), findsOneWidget);
      expect(find.text('I am on the migration.'), findsOneWidget, reason: 'the conversation is the main view');
      expect(find.text('Message Forge (owner)…'), findsOneWidget);
      expect(find.byKey(const ValueKey('ticket-segments')), findsNothing, reason: 'no tabs any more');
      expect(find.byKey(const ValueKey('gate-plan')), findsNothing);
      expect(find.byKey(const ValueKey('gate-review')), findsNothing);
      expect(find.byKey(const ValueKey('bar-backlog')), findsNothing, reason: 'the status box says nothing while the work runs');
    });

    testWidgets('a main task that stopped on a failure: the status box says so, and Try again asks the owner again', (tester) async {
      final stopped = taskOf(id: 'root', status: TaskStatus.waiting, waitingReason: 'input', employeeId: 'forge', employeeName: 'Forge', conversationId: 'c-root');
      await open(tester, _ticket(rootTaskStatus: TaskStatus.waiting), root: stopped, rootMessages: [
        messageOf('u1', MessageRole.user, 'Add the users table.'),
        messageOf('a1', MessageRole.assistant, 'The runtime crashed. Send the message again to retry.', status: MessageStatus.failed),
      ]);
      expect(find.text('Forge stopped and waits for you.'), findsOneWidget);
      expect(find.byKey(const ValueKey('ticket-stopped-failure')), findsOneWidget);
      expect(find.text('The runtime crashed.'), findsOneWidget);
      await tapKey(tester, 'bar-try-again');
      final write = writesOf(repo).single;
      expect(write['method'], 'employee.send_message');
      expect(paramsOf(write), containsPair('text', 'Add the users table.'));
      expect(paramsOf(write), containsPair('conversation_id', 'c-root'));
      expect(paramsOf(write), containsPair('employee_id', 'forge'));
    });

    testWidgets('Mode and Model under the input write ticket.update { run } with the version', (tester) async {
      await open(tester, _ticket(rootTaskStatus: TaskStatus.queued, run: const TaskRun(model: 'claude-sonnet-5-5')));
      expect(find.text('Mode · Default'), findsOneWidget);
      expect(find.text('Model · Sonnet 5.5'), findsOneWidget);
      await tapKey(tester, 'chip-mode');
      await tester.tap(find.byKey(const ValueKey('mode-auto')));
      await settleBoard(tester);
      final mode = verify(() => repo.write(captureAny(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: 3, retry: true)).captured.single as Map<String, dynamic>;
      expect(mode['method'], 'ticket.update');
      expect(paramsOf(mode)['run'], {'mode': 'auto', 'model': 'claude-sonnet-5-5'});

      when(() => repo.connections()).thenAnswer((_) async => const ConnectionsSnapshot(asOfSeq: 1, defaults: [], connections: [
            ConnectionView(authKind: 'oauth', billing: 'subscription', displayName: 'Claude', hasSecret: false, id: 'c1', models: ['claude-opus-5-5', 'claude-sonnet-5-5'], provider: 'anthropic', status: 'ok'),
          ]));
      await tapKey(tester, 'chip-model');
      await tester.tap(find.byKey(const ValueKey('model-claude-opus-5-5')));
      await settleBoard(tester);
      final model = verify(() => repo.write(captureAny(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: 3, retry: true)).captured.single as Map<String, dynamic>;
      expect(paramsOf(model)['run'], {'model': 'claude-opus-5-5'});
    });

    testWidgets('while the main task runs the run chips are hidden (runLocked)', (tester) async {
      await open(tester, _ticket(rootTaskStatus: TaskStatus.running));
      expect(find.byKey(const ValueKey('chip-mode')), findsNothing);
      expect(find.byKey(const ValueKey('chip-model')), findsNothing);
      expect(find.byKey(const ValueKey('ticket-hint')), findsOneWidget);
    });
  });

  group('Review', () {
    final reported = taskOf(id: 'root', status: TaskStatus.review, result: 'Hashing done. Branch: feat/argon2', employeeId: 'forge', employeeName: 'Forge', conversationId: 'c-root');

    testWidgets('the gate has the report, Done and Send back; after Done the session reads the ticket again and turns read-only without a reload', (tester) async {
      await open(tester, _ticket(column: ColumnKind.review, rootTaskStatus: TaskStatus.review), root: reported, messages: [_line('s1', SystemLineKind.review, 'Forge reported.', taskId: 'root')]);
      expect(find.text('In review · your decision'), findsOneWidget);
      expect(find.textContaining('Branch: feat/argon2'), findsWidgets);
      expect(find.byKey(const ValueKey('bar-send-back')), findsOneWidget);
      // The server's answer to the next read: the ticket is Done.
      when(() => repo.ticket(any())).thenAnswer((_) async => TicketDetail(asOfSeq: 2, ticket: _ticket(column: ColumnKind.done, rootTaskStatus: TaskStatus.succeeded), rootTask: reported, tasks: [reported]));
      when(() => repo.jobs()).thenAnswer((_) async => const []);
      await tapKey(tester, 'bar-done');
      // The confirmation reads the ticket fresh (B6); it is still in Review there.
      when(() => repo.ticket(any())).thenAnswer((_) async => TicketDetail(asOfSeq: 2, ticket: _ticket(column: ColumnKind.done, rootTaskStatus: TaskStatus.succeeded), rootTask: reported, tasks: [reported]));
      await tester.tap(find.byKey(const ValueKey('mark-done')));
      await settleBoard(tester);
      expect(writesOf(repo).single['method'], 'ticket.accept');
      expect(find.text('#42 is Done · read-only.'), findsOneWidget);
      expect(find.byKey(const ValueKey('gate-review')), findsNothing);
      expect(find.byKey(const Key('composer-send')), findsNothing);
      expect(find.descendant(of: find.byKey(const ValueKey('ticket-column')), matching: find.text('Done')), findsOneWidget);
    });

    testWidgets('Send back takes the words in the input and sends ticket.send_back', (tester) async {
      await open(tester, _ticket(column: ColumnKind.review, rootTaskStatus: TaskStatus.review), root: reported, messages: [_line('s1', SystemLineKind.review, 'Forge reported.', taskId: 'root')]);
      await tester.enterText(find.byType(TextField).last, 'Use argon2id.');
      await tester.pump();
      await tapKey(tester, 'bar-send-back');
      final write = writesOf(repo).single;
      expect(write['method'], 'ticket.send_back');
      expect(paramsOf(write), {'ticket_id': 'k1', 'feedback': 'Use argon2id.'});
      expect(tester.widget<TextField>(find.byType(TextField).last).controller!.text, isEmpty);
    });
  });

  group('Plan review', () {
    final waiting = taskOf(id: 'root', status: TaskStatus.waiting, waitingReason: 'plan_review', employeeId: 'forge', employeeName: 'Forge', conversationId: 'c-root');
    final planned = [messageOf('p1', MessageRole.assistant, 'Plan:\n1. Add the users table', update: 'plan')];
    final line = [_line('s1', SystemLineKind.plan, 'Forge handed in a plan.', taskId: 'root')];

    testWidgets('the gate shows the plan; Approve, Approve accept edits and Reject use the existing RPCs and confirmation', (tester) async {
      await open(tester, _ticket(column: ColumnKind.planReview, rootTaskStatus: TaskStatus.waiting, run: const TaskRun(mode: AgentMode.plan)), root: waiting, rootMessages: planned, messages: line);
      expect(find.text('Plan review · your decision'), findsOneWidget);
      expect(find.byKey(const ValueKey('gate-plan-text')), findsOneWidget);
      await tapKey(tester, 'gate-approve');
      expect(paramsOf(writesOf(repo).single), {'ticket_id': 'k1', 'decision': 'approve'});
      clearInteractions(repo);
      await tapKey(tester, 'gate-approve-edits');
      expect(paramsOf(writesOf(repo).single)['decision'], 'approve_edits');
      clearInteractions(repo);
      await tapKey(tester, 'gate-reject');
      expect(find.text('Reject the plan of #42?'), findsOneWidget);
      await tester.tap(find.descendant(of: find.byType(AlertDialog), matching: find.widgetWithText(TextButton, 'Reject')));
      await settleBoard(tester);
      expect(writesOf(repo).single['method'], 'ticket.cancel');
    });
  });

  group('Done and archived (BR5)', () {
    testWidgets('Done: the read-only bar, no input, no gate, no run chips; the timeline stays readable', (tester) async {
      await open(tester, _ticket(column: ColumnKind.done, rootTaskStatus: TaskStatus.succeeded), messages: [_post('m1', 'Shipped.', author: 'Forge')]);
      expect(find.text('#42 is Done · read-only.'), findsOneWidget);
      expect(find.text('Shipped.'), findsOneWidget);
      expect(find.byKey(const Key('composer-send')), findsNothing);
      expect(find.byKey(const ValueKey('chip-mode')), findsNothing);
      expect(find.byKey(const ValueKey('gate-review')), findsNothing);
      expect(find.byKey(const ValueKey('ticket-backlog')), findsNothing);
    });

    testWidgets('archived: the archived bar', (tester) async {
      await open(tester, _ticket(column: ColumnKind.doing, archivedAtMs: 5));
      expect(find.text('#42 is archived · read-only.'), findsOneWidget);
      expect(find.byKey(const Key('composer-send')), findsNothing);
    });
  });

  group('Backlog', () {
    testWidgets('Start, Start with… and Delete; Delete asks, then archives', (tester) async {
      await open(tester, _ticket(column: ColumnKind.backlog));
      expect(find.byKey(const ValueKey('bar-start')), findsOneWidget);
      expect(find.byKey(const ValueKey('bar-start-with')), findsOneWidget);
      await tapKey(tester, 'bar-delete');
      expect(find.text('Delete #42?'), findsOneWidget);
      await tester.tap(find.descendant(of: find.byType(AlertDialog), matching: find.widgetWithText(TextButton, 'Delete')));
      await settleBoard(tester);
      expect(writesOf(repo).single['method'], 'ticket.archive');
      expect(find.text('#42 is deleted.'), findsOneWidget);
    });
  });

  group('the properties sheet', () {
    testWidgets('has the ticket data and the task tree, and its Activity', (tester) async {
      await open(tester, _ticket(responsible: bo), activity: [cardOf('ticket.started', seq: 1)]);
      await openTicketProperties(tester);
      for (final row in ['ticket-row-priority', 'ticket-row-owner', 'ticket-row-responsible', 'ticket-row-mode', 'ticket-row-labels', 'ticket-row-deadline', 'ticket-row-project', 'ticket-row-files']) {
        expect(find.byKey(ValueKey(row)), findsOneWidget, reason: row);
      }
      await tester.scrollUntilVisible(find.byKey(const ValueKey('ticket-task-root')), 200, scrollable: find.descendant(of: find.byKey(const ValueKey('ticket-page')), matching: find.byType(Scrollable)).first);
      expect(find.textContaining('Main task ·'), findsOneWidget);
      await tester.tap(find.descendant(of: find.byKey(const ValueKey('ticket-segments')), matching: find.text('Activity')));
      await settleBoard(tester);
      expect(find.text('A board member started it'), findsOneWidget);
    });
  });

  group('links and edges', () {
    testWidgets('the old ?tab=activity opens the Activity sheet', (tester) async {
      final ticket = _ticket();
      await open(tester, ticket, page: TicketPage(ticketId: ticket.id, activity: true), activity: [cardOf('ticket.started', seq: 1)]);
      expect(find.text('Ticket properties'), findsOneWidget);
      expect(find.text('A board member started it'), findsOneWidget);
    });

    testWidgets('the old ?tab=discussion lands in the session', (tester) async {
      final ticket = _ticket();
      await open(tester, ticket, page: TicketPage(ticketId: ticket.id, discussion: true));
      expect(find.byKey(const Key('composer-send')), findsOneWidget);
      expect(find.text('Ticket properties'), findsNothing);
    });

    testWidgets('a number resolves through the ticket channels; an unknown one says "Ticket #N not found."', (tester) async {
      const channel = ChannelView(archived: false, conversationId: 'disc-1', kind: ChannelKind.ticket, mentions: 0, name: '#42-argon2', ticketId: 'k1', ticketNumber: 42, unread: 0);
      final ticket = _ticket();
      repo = boardRepository(detail: TicketDetail(asOfSeq: 1, ticket: ticket, tasks: const []), link: _session);
      when(() => repo.searchChannels(any())).thenAnswer((i) async => i.positionalArguments.first == '#42' ? const [channel] : const []);
      when(() => repo.messages(any())).thenAnswer((_) async => snapshotOf('disc-1'));
      when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.appends).thenAnswer((_) => const Stream.empty());
      when(() => repo.loadChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.load()).thenAnswer((_) async => loadOf());
      await pumpBoardPage(tester, repo, const TicketPage(ticketId: '42'), link: _session);
      verify(() => repo.ticket('k1')).called(greaterThan(0));
      expect(find.text('Hash passwords with argon2'), findsOneWidget);

      await pumpBoardPage(tester, repo, const TicketPage(ticketId: '9999'), link: _session);
      expect(find.text('Ticket #9999 not found.'), findsOneWidget);
      expect(find.byKey(const ValueKey('ticket-go-to-board')), findsOneWidget);
    });

    testWidgets('a number on a server without ticket channels cannot be found, and reads nothing', (tester) async {
      repo = boardRepository();
      await pumpBoardPage(tester, repo, const TicketPage(ticketId: '42'));
      expect(find.text('Ticket #42 not found.'), findsOneWidget);
      verifyNever(() => repo.ticket(any()));
    });

    testWidgets('a failed read shows what failed and Try again, never an endless loading', (tester) async {
      repo = boardRepository(link: _session);
      var calls = 0;
      when(() => repo.ticket(any())).thenAnswer((_) async {
        if (calls++ == 0) throw const ProtocolException(ErrorCode.internal, 'The ticket could not be read.');
        return TicketDetail(asOfSeq: 1, ticket: _ticket(), tasks: const []);
      });
      when(() => repo.messages(any())).thenAnswer((_) async => snapshotOf('disc-1'));
      when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.appends).thenAnswer((_) => const Stream.empty());
      when(() => repo.loadChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.load()).thenAnswer((_) async => loadOf());
      await pumpBoardPage(tester, repo, const TicketPage(ticketId: 'k1'), link: _session);
      expect(find.byType(CircularProgressIndicator), findsNothing);
      expect(find.text('Try again'), findsOneWidget);
      await tester.tap(find.text('Try again'));
      await settleBoard(tester);
      expect(find.text('Hash passwords with argon2'), findsOneWidget);
    });
  });
}
