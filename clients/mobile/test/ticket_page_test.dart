import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/board/ticket_page.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  TicketDetail detailOf(TicketView ticket, {TaskView? root, List<TaskView> tasks = const []}) =>
      TicketDetail(asOfSeq: 1, ticket: ticket, rootTask: root, tasks: [?root, ...tasks]);

  Future<MockRepository> open(WidgetTester tester, TicketDetail detail, {List<TimelineCard> activity = const [], MessagesSnapshot? messages, CompanyState? company, bool plan = false}) async {
    final repo = boardRepository(detail: detail, activity: activity, messages: messages);
    await pumpBoardPage(tester, repo, plan ? TicketPlanPage(ticketId: detail.ticket.id) : TicketPage(ticketId: detail.ticket.id), company: company);
    return repo;
  }

  Future<void> tapKey(WidgetTester tester, String key) async {
    await tester.ensureVisible(find.byKey(ValueKey(key)));
    await tester.pump();
    await tester.tap(find.byKey(ValueKey(key)));
    await settleBoard(tester);
  }

  group('a ticket in Backlog', () {
    testWidgets('the details: priority, owner (the dispatcher), mode, labels, created by (MU1-07), in the properties', (tester) async {
      await open(tester, detailOf(ticketOf(number: 51, title: 'Vault UI', priority: TicketPriority.urgent, labels: const ['ui', 'vault'], description: 'Board members log in.')));
      expect(find.byKey(const ValueKey('session-kind-ticket')), findsOneWidget);
      expect(tester.widget<Text>(find.byKey(const ValueKey('session-head-detail'))).data, '#51 · Backlog');
      expect(find.text('Vault UI'), findsWidgets);
      expect(find.text('This server has no ticket conversation.'), findsOneWidget, reason: 'a server without ticket conversations');
      await openTicketProperties(tester);
      expect(find.text('▲▲ Urgent'), findsOneWidget);
      expect(find.text('Dispatcher (Orion)'), findsOneWidget);
      expect(find.text('ui, vault'), findsOneWidget);
      expect(find.textContaining('ada ·'), findsOneWidget, reason: 'the creator, never editable');
      await tester.scrollUntilVisible(find.text('Board members log in.'), 200, scrollable: find.descendant(of: find.byKey(const ValueKey('ticket-page')), matching: find.byType(Scrollable)).first);
      expect(find.text('Board members log in.'), findsOneWidget);
    });

    testWidgets('choose an owner, and give it back to the dispatcher (ticket.assign without employee_id)', (tester) async {
      final repo = await open(tester, detailOf(ticketOf(ownerEmployeeId: 'forge', ownerName: 'Forge')));
      await openTicketProperties(tester);
      await tapKey(tester, 'ticket-row-owner');
      expect(find.text('Dispatcher (Orion)'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('owner-dispatcher')));
      await settleBoard(tester);
      final assign = writesOf(repo).single;
      expect(assign['method'], 'ticket.assign');
      expect(paramsOf(assign), {'ticket_id': 'k1'}, reason: 'no employee: back to the dispatcher');
      expect(find.text('The dispatcher chooses the owner of #1.'), findsOneWidget);
    });

    testWidgets('Start with the owner: the toast names it, no Undo (MU1-12 AC1)', (tester) async {
      final repo = await open(tester, detailOf(ticketOf(number: 8, ownerEmployeeId: 'forge', ownerName: 'Forge')));
      await tapKey(tester, 'bar-start');
      expect(writesOf(repo).single['method'], 'ticket.start');
      expect(find.text('#8 started with Forge.'), findsOneWidget);
      expect(find.text('Undo'), findsNothing);
    });

    testWidgets('Start with…: skips the dispatcher (ticket.start with employee_id)', (tester) async {
      final repo = await open(tester, detailOf(ticketOf()));
      await tapKey(tester, 'bar-start-with');
      await tester.tap(find.byKey(const ValueKey('start-with-forge')));
      await settleBoard(tester);
      final start = writesOf(repo).single;
      expect(paramsOf(start), {'ticket_id': 'k1', 'employee_id': 'forge'});
    });

    testWidgets('priority (B9): a sheet with the four, saved with the version', (tester) async {
      final repo = await open(tester, detailOf(ticketOf()));
      await openTicketProperties(tester);
      await tapKey(tester, 'ticket-row-priority');
      await tester.tap(find.byKey(const ValueKey('priority-high')));
      await settleBoard(tester);
      final update = verify(() => repo.write(captureAny(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: 3, retry: true)).captured.single as Map<String, dynamic>;
      expect(update['method'], 'ticket.update');
      expect(paramsOf(update), {'ticket_id': 'k1', 'priority': 'high'});
    });

    testWidgets('labels: one field, split on commas', (tester) async {
      final repo = await open(tester, detailOf(ticketOf(labels: const ['ui'])));
      await openTicketProperties(tester);
      await tapKey(tester, 'ticket-row-labels');
      await tester.enterText(find.byType(TextField), 'ui, backend ,security');
      await tester.tap(find.text('Save'));
      await settleBoard(tester);
      expect(paramsOf(writesOf(repo).single), {'ticket_id': 'k1', 'labels': ['ui', 'backend', 'security']});
    });
  });

  group('Todo', () {
    testWidgets('nobody dispatches: "Needs you · choose an owner" and Choose employee (B5)', (tester) async {
      final repo = await open(tester, detailOf(ticketOf(column: ColumnKind.todo)), activity: [cardOf('ticket.updated', actor: 'system', seq: 2), cardOf('ticket.started', seq: 1)]);
      expect(find.byKey(const ValueKey('ticket-needs-owner')), findsOneWidget);
      expect(find.text('Orion could not choose an employee.'), findsOneWidget);
      await tapKey(tester, 'bar-choose');
      await tester.tap(find.byKey(const ValueKey('owner-forge')));
      await settleBoard(tester);
      expect(paramsOf(writesOf(repo).single), {'ticket_id': 'k1', 'employee_id': 'forge'});
    });

    testWidgets('without a dispatcher at all, it says so', (tester) async {
      await open(tester, detailOf(ticketOf(column: ColumnKind.todo)), company: companyWith(dispatcher: null));
      expect(find.text('No dispatcher is set for this project. Started tickets wait in Todo until you choose an owner.'), findsOneWidget);
    });

    testWidgets("the project's dispatcher archived: needs you, the company's does not take over (B5 strict)", (tester) async {
      final gone = employeeOf(id: 'gone', name: 'Vesta', status: LifecycleStatus.archived);
      await open(tester, detailOf(ticketOf(column: ColumnKind.todo)), company: companyWith(projectDispatcher: 'gone', employees: [orion, forge, gone]));
      expect(find.byKey(const ValueKey('ticket-needs-owner')), findsOneWidget, reason: 'no timeline needed: nobody chooses');
      expect(find.textContaining('is choosing an employee'), findsNothing);
      expect(find.text('Vesta is archived. Started tickets wait in Todo until you choose an owner.'), findsOneWidget);
      await openTicketProperties(tester);
      expect(find.text('None · Vesta is archived'), findsOneWidget);
      expect(find.textContaining('Orion'), findsNothing);
    });

    testWidgets("Start with the project's dispatcher paused: the toast says nobody chooses, no Orion (B5 strict)", (tester) async {
      final resting = employeeOf(id: 'resting', name: 'Vesta', status: LifecycleStatus.paused);
      await open(tester, detailOf(ticketOf(number: 9)), company: companyWith(projectDispatcher: 'resting', employees: [orion, forge, resting]));
      await tapKey(tester, 'bar-start');
      expect(find.text('#9 started. Choose an owner: Vesta is paused.'), findsOneWidget);
    });
  });

  group('Doing', () {
    testWidgets('the mode is locked while the main task runs (MU1-07 AC5); the task tree opens the task (MU1-08)', (tester) async {
      final root = taskOf(id: 'root', title: 'Login per board member', status: TaskStatus.running, employeeName: 'Forge', conversationId: 'conv-root');
      final sub = taskOf(id: 'sub', title: 'Users table', status: TaskStatus.succeeded, parentTaskId: 'root', employeeName: 'Schema');
      final repo = await open(tester, detailOf(ticketOf(column: ColumnKind.doing, rootTaskId: 'root', ownerEmployeeId: 'forge', ownerName: 'Forge', run: const TaskRun(mode: AgentMode.auto)), root: root, tasks: [sub]));
      await openTicketProperties(tester);
      await tapKey(tester, 'ticket-row-mode');
      expect(find.text('Locked while the main task runs.'), findsOneWidget);
      verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
      expect(find.text('Users table'), findsOneWidget);
      await tapKey(tester, 'ticket-task-sub');
      expect(find.text('task sub'), findsOneWidget);
    });

    testWidgets('a failed main task stays in Doing: Start again (B6)', (tester) async {
      final root = taskOf(id: 'root', status: TaskStatus.failed, result: 'The build broke.');
      final repo = await open(tester, detailOf(ticketOf(number: 44, column: ColumnKind.doing, rootTaskId: 'root', rootTaskStatus: TaskStatus.failed, ownerEmployeeId: 'forge', ownerName: 'Forge'), root: root));
      expect(find.byKey(const ValueKey('ticket-failed')), findsOneWidget);
      expect(find.text('The build broke.'), findsOneWidget);
      await tapKey(tester, 'bar-start-again');
      expect(writesOf(repo).single['method'], 'ticket.start');
      expect(find.text('#44 starts again.'), findsOneWidget);
    });

    testWidgets('a failed main task in Todo shows failed too: Start again, no Undo (as the desktop)', (tester) async {
      final root = taskOf(id: 'root', status: TaskStatus.failed);
      final repo = await open(tester, detailOf(ticketOf(number: 45, column: ColumnKind.todo, rootTaskId: 'root', rootTaskStatus: TaskStatus.failed), root: root));
      expect(find.byKey(const ValueKey('ticket-failed')), findsOneWidget);
      expect(find.textContaining('is choosing an employee'), findsNothing);
      await tapKey(tester, 'bar-start-again');
      expect(writesOf(repo).single['method'], 'ticket.start');
      expect(find.text('#45 starts again.'), findsOneWidget);
      expect(find.text('Undo'), findsNothing);
    });

    testWidgets('while the work runs the gate says nothing; Back to Backlog in More asks first, then cancels', (tester) async {
      final root = taskOf(id: 'root', status: TaskStatus.running);
      final repo = await open(tester, detailOf(ticketOf(column: ColumnKind.doing, rootTaskId: 'root', ownerEmployeeId: 'forge'), root: root));
      expect(find.byKey(const ValueKey('bar-backlog')), findsNothing, reason: 'the desktop session: the timeline says what runs');
      await tester.tap(find.byTooltip('Ticket actions'));
      await settleBoard(tester);
      await tester.tap(find.text('Back to Backlog'));
      await settleBoard(tester);
      expect(find.text('Back to Backlog?'), findsOneWidget);
      await tester.tap(find.text('Cancel'));
      await settleBoard(tester);
      verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
    });
  });

  group('Review (MU1-16, MU1-17)', () {
    final root = taskOf(id: 'root', title: 'Argon2', status: TaskStatus.review, result: 'Hashing done. Branch: feat/argon2', employeeName: 'Sentinel');
    TicketDetail review({List<TaskView> tasks = const []}) =>
        detailOf(ticketOf(number: 40, column: ColumnKind.review, rootTaskId: 'root', ownerEmployeeId: 'forge', ownerName: 'Sentinel'), root: root, tasks: tasks);

    testWidgets('the report comes first, as information: no merge button', (tester) async {
      await open(tester, review());
      expect(find.byKey(const ValueKey('ticket-report')), findsOneWidget);
      expect(find.textContaining('Branch: feat/argon2'), findsOneWidget);
      expect(find.textContaining('Merge'), findsNothing);
    });

    testWidgets('Send back needs feedback (B2): off while empty, then ticket.send_back with the text', (tester) async {
      final repo = await open(tester, review());
      await tapKey(tester, 'bar-send-back');
      expect(find.text('Send #40 back to Doing'), findsOneWidget);
      expect(tester.widget<TextButton>(find.byKey(const ValueKey('feedback-send'))).onPressed, isNull, reason: 'MU1-16 AC3');
      await tester.enterText(find.byKey(const ValueKey('feedback-field')), '  Use argon2id with m=64MiB.  ');
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('feedback-send')));
      await settleBoard(tester);
      final send = verify(() => repo.write(captureAny(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: 3, retry: false)).captured.single as Map<String, dynamic>;
      expect(send['method'], 'ticket.send_back');
      expect(paramsOf(send), {'ticket_id': 'k1', 'feedback': 'Use argon2id with m=64MiB.'});
    });

    testWidgets('an empty submit puts the error under the field', (tester) async {
      await open(tester, review());
      await tapKey(tester, 'bar-send-back');
      await tester.enterText(find.byKey(const ValueKey('feedback-field')), '   ');
      await tester.testTextInput.receiveAction(TextInputAction.done);
      await tester.pump();
      expect(find.text('Write what needs to change.'), findsOneWidget);
    });

    testWidgets('Done asks once, says nothing is merged, then ticket.accept (D-08)', (tester) async {
      final repo = await open(tester, review());
      await tapKey(tester, 'bar-done');
      expect(find.text('Mark #40 as Done?'), findsOneWidget);
      expect(find.textContaining('Nothing is merged, pushed or deployed.'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('mark-done')));
      await settleBoard(tester);
      expect(writesOf(repo).single['method'], 'ticket.accept');
      expect(find.text('#40 is Done. A summary goes to project memory.'), findsOneWidget);
    });

    testWidgets('Done waits while a delegation still runs (B6)', (tester) async {
      final repo = await open(tester, review(tasks: [taskOf(id: 'd', title: 'Review params', status: TaskStatus.running, parentTaskId: 'root', employeeName: 'Schema')]));
      await tapKey(tester, 'bar-done');
      expect(find.textContaining('1 task is still running'), findsOneWidget);
      expect(find.textContaining('Review params'), findsWidgets);
      expect(tester.widget<FilledButton>(find.byKey(const ValueKey('mark-done'))).onPressed, isNull);
      await tester.tap(find.text('Cancel'));
      await settleBoard(tester);
      verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
    });

    testWidgets('a delegation in review or draft does not hold Done (B6, as the server counts)', (tester) async {
      final repo = await open(tester, review(tasks: [
        taskOf(id: 'd1', title: 'Check the report', status: TaskStatus.review, parentTaskId: 'root'),
        taskOf(id: 'd2', title: 'Write the docs', status: TaskStatus.draft, parentTaskId: 'root'),
      ]));
      await tapKey(tester, 'bar-done');
      expect(find.textContaining('still running'), findsNothing);
      await tester.tap(find.byKey(const ValueKey('mark-done')));
      await settleBoard(tester);
      expect(writesOf(repo).single['method'], 'ticket.accept');
    });

    JobSummary jobIn(String conversation, JobState state) =>
        JobSummary(conversationId: conversation, createdAtMs: 0, id: 'j-$conversation-${state.wire}', kind: 'conversation.answer', label: 'Answer', progressPermille: 0, state: state);

    testWidgets("a run in the main task's conversation holds Done, as the server (B6: a question asked in Review)", (tester) async {
      for (final state in [JobState.leased, JobState.ready]) {
        final talking = taskOf(id: 'root', title: 'Argon2', status: TaskStatus.review, conversationId: 'c-root', employeeName: 'Sentinel');
        final repo = boardRepository(
          detail: detailOf(ticketOf(number: 40, column: ColumnKind.review, rootTaskId: 'root', ownerEmployeeId: 'forge'), root: talking),
          jobs: [jobIn('c-other', JobState.leased), jobIn('c-root', state)],
        );
        await pumpBoardPage(tester, repo, TicketPage(ticketId: 'k1'));
        await tapKey(tester, 'bar-done');
        expect(find.byKey(const ValueKey('mark-done-conversation')), findsOneWidget, reason: state.wire);
        expect(tester.widget<FilledButton>(find.byKey(const ValueKey('mark-done'))).onPressed, isNull, reason: state.wire);
        await tester.tap(find.text('Cancel'));
        await settleBoard(tester);
        verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
      }
    });

    testWidgets("a finished job in the main task's conversation, or a run elsewhere, does not hold Done (B6)", (tester) async {
      final quiet = taskOf(id: 'root', title: 'Argon2', status: TaskStatus.review, conversationId: 'c-root');
      final repo = boardRepository(
        detail: detailOf(ticketOf(number: 40, column: ColumnKind.review, rootTaskId: 'root', ownerEmployeeId: 'forge'), root: quiet),
        jobs: [jobIn('c-root', JobState.done), jobIn('c-other', JobState.leased)],
      );
      await pumpBoardPage(tester, repo, TicketPage(ticketId: 'k1'));
      await tapKey(tester, 'bar-done');
      expect(find.byKey(const ValueKey('mark-done-conversation')), findsNothing);
      await tester.tap(find.byKey(const ValueKey('mark-done')));
      await settleBoard(tester);
      expect(writesOf(repo).single['method'], 'ticket.accept');
    });

    testWidgets('Done stays off when the ticket cannot be read again (ticket.get fails)', (tester) async {
      final repo = await open(tester, review());
      when(() => repo.ticket(any())).thenThrow(const ProtocolException(ErrorCode.internal, 'ticket failed'));
      await tapKey(tester, 'bar-done');
      expect(find.byKey(const ValueKey('mark-done-unknown')), findsOneWidget);
      expect(tester.widget<FilledButton>(find.byKey(const ValueKey('mark-done'))).onPressed, isNull);
    });

    testWidgets('Done stays off when the jobs cannot be read', (tester) async {
      final talking = taskOf(id: 'root', title: 'Argon2', status: TaskStatus.review, conversationId: 'c-root');
      final repo = boardRepository(detail: detailOf(ticketOf(number: 40, column: ColumnKind.review, rootTaskId: 'root', ownerEmployeeId: 'forge'), root: talking));
      when(() => repo.jobs()).thenThrow(const ProtocolException(ErrorCode.internal, 'jobs failed'));
      await pumpBoardPage(tester, repo, TicketPage(ticketId: 'k1'));
      await tapKey(tester, 'bar-done');
      expect(find.byKey(const ValueKey('mark-done-unknown')), findsOneWidget);
      expect(tester.widget<FilledButton>(find.byKey(const ValueKey('mark-done'))).onPressed, isNull);
    });

    testWidgets('queued and waiting delegations hold Done (B6)', (tester) async {
      final repo = await open(tester, review(tasks: [
        taskOf(id: 'd1', title: 'Queued work', status: TaskStatus.queued, parentTaskId: 'root'),
        taskOf(id: 'd2', title: 'Waiting work', status: TaskStatus.waiting, parentTaskId: 'root'),
        taskOf(id: 'd3', title: 'Draft work', status: TaskStatus.draft, parentTaskId: 'root'),
      ]));
      await tapKey(tester, 'bar-done');
      expect(find.textContaining('2 tasks are still running'), findsOneWidget);
      expect(tester.widget<FilledButton>(find.byKey(const ValueKey('mark-done'))).onPressed, isNull);
      await tester.tap(find.text('Cancel'));
      await settleBoard(tester);
      verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
    });
  });

  testWidgets('Todo: an activity that cannot be read says so and offers Try again, not "choosing…" (M-m3)', (tester) async {
    final repo = boardRepository(detail: detailOf(ticketOf(column: ColumnKind.todo)));
    var calls = 0;
    when(() => repo.timeline(correlationId: any(named: 'correlationId'), activity: any(named: 'activity'), limit: any(named: 'limit'))).thenAnswer((_) async {
      if (calls++ == 0) throw const ProtocolException(ErrorCode.internal, 'timeline failed');
      return TimelinePage(cards: [cardOf('ticket.updated', actor: 'system', seq: 3), cardOf('ticket.started', seq: 2)]);
    });
    await pumpBoardPage(tester, repo, TicketPage(ticketId: 'k1'));
    expect(find.byKey(const ValueKey('ticket-owner-unknown')), findsOneWidget);
    expect(find.textContaining('is choosing an employee'), findsNothing);
    expect(find.byKey(const ValueKey('bar-choose')), findsOneWidget);
    await tester.tap(find.text('Try again'));
    await settleBoard(tester);
    expect(find.byKey(const ValueKey('ticket-owner-unknown')), findsNothing);
    expect(find.byKey(const ValueKey('ticket-needs-owner')), findsOneWidget, reason: 'read again: the dispatcher gave up');
  });

  testWidgets('Done: read only, with what did not happen (MU1-16 AC1, MU1-17)', (tester) async {
    await open(tester, detailOf(ticketOf(column: ColumnKind.done, doneAtMs: DateTime(2026, 10, 31, 11, 2).millisecondsSinceEpoch)));
    expect(find.text('#1 is Done · read-only.'), findsOneWidget);
    await openTicketProperties(tester);
    expect(find.byKey(const ValueKey('ticket-done')), findsOneWidget);
    expect(find.text('Nothing was merged, pushed or deployed.'), findsOneWidget);
    expect(find.byKey(const ValueKey('bar-start')), findsNothing);
    await tester.tap(find.byKey(const ValueKey('ticket-row-priority')));
    await settleBoard(tester);
    expect(find.byKey(const ValueKey('priority-high')), findsNothing, reason: 'nothing to edit');
  });

  testWidgets('Activity: who did what, in words (MU1-15 AC4, MU1-14 AC3)', (tester) async {
    await open(tester, detailOf(ticketOf(column: ColumnKind.doing)), activity: [cardOf('ticket.plan_approved', seq: 3), cardOf('ticket.dispatched', actor: 'employee', seq: 2), cardOf('ticket.started', seq: 1)]);
    await openTicketProperties(tester, activity: true);
    expect(find.text('A board member approved the plan'), findsOneWidget);
    expect(find.text('The dispatcher chose an owner'), findsOneWidget);
    expect(find.text('A board member started it'), findsOneWidget);
  });

  group('Plan review (§9.1, MU1-13/14)', () {
    final root = taskOf(id: 'root', status: TaskStatus.waiting, waitingReason: 'plan_review', conversationId: 'conv-root');
    final plan = snapshotOf('conv-root', messages: [
      messageOf('m1', MessageRole.user, 'Do it'),
      messageOf('m2', MessageRole.assistant, 'Plan:\n1. Add the users table\n2. Hash with argon2', update: 'plan'),
      messageOf('m3', MessageRole.assistant, 'I stop here.'),
    ]);
    TicketDetail planned() => detailOf(ticketOf(number: 42, column: ColumnKind.planReview, rootTaskId: 'root', ownerEmployeeId: 'forge', ownerName: 'Forge', run: const TaskRun(mode: AgentMode.plan)), root: root);

    testWidgets('the gate shows the plan and opens the whole plan', (tester) async {
      await open(tester, planned(), messages: plan);
      expect(find.text('Plan review · your decision'), findsOneWidget);
      expect(tester.widget<Text>(find.byKey(const ValueKey('session-head-detail'))).data, '#42 · ◆ Plan to review');
      expect(find.textContaining('Hash with argon2'), findsOneWidget);
      await tapKey(tester, 'gate-whole-plan');
      expect(find.text('plan k1'), findsOneWidget);
    });

    testWidgets("the plan is the main task's plan update; Approve approves it in Auto (D-07)", (tester) async {
      final repo = await open(tester, planned(), messages: plan, plan: true);
      expect(find.textContaining('Hash with argon2'), findsOneWidget);
      expect(find.text('I stop here.'), findsNothing);
      await tester.tap(find.byKey(const ValueKey('ticket-plan-approve')));
      await settleBoard(tester);
      final approve = writesOf(repo).single;
      expect(approve['method'], 'ticket.approve_plan');
      expect(paramsOf(approve), {'ticket_id': 'k1', 'decision': 'approve'});
    });

    testWidgets('after the approval Mode reads "Plan → Auto" (D-07), from the main task\'s conversation', (tester) async {
      final doing = detailOf(ticketOf(number: 42, column: ColumnKind.doing, rootTaskId: 'root', ownerEmployeeId: 'forge', ownerName: 'Forge', run: const TaskRun(mode: AgentMode.plan)), root: taskOf(id: 'root', status: TaskStatus.queued, conversationId: 'conv-root'));
      await open(tester, doing, messages: snapshotOf('conv-root', agentMode: AgentMode.auto), activity: [cardOf('ticket.plan_approved', seq: 4), cardOf('ticket.started', seq: 2)]);
      await openTicketProperties(tester);
      await tester.ensureVisible(find.byKey(const ValueKey('ticket-row-mode')));
      await tester.pump();
      expect(find.text('Plan → Auto'), findsOneWidget);
    });

    testWidgets('Approve with edits reads "Plan → Accept edits"; before any approval it stays "Plan"', (tester) async {
      final doing = detailOf(ticketOf(number: 42, column: ColumnKind.doing, rootTaskId: 'root', ownerEmployeeId: 'forge', run: const TaskRun(mode: AgentMode.plan)), root: taskOf(id: 'root', status: TaskStatus.running, conversationId: 'conv-root'));
      await open(tester, doing, messages: snapshotOf('conv-root', agentMode: AgentMode.acceptEdits), activity: [cardOf('ticket.plan_approved', seq: 4)]);
      await openTicketProperties(tester);
      await tester.ensureVisible(find.byKey(const ValueKey('ticket-row-mode')));
      await tester.pump();
      expect(find.text('Plan → Accept edits'), findsOneWidget);

      await open(tester, doing, messages: snapshotOf('conv-root', agentMode: AgentMode.auto), activity: [cardOf('ticket.started', seq: 2)]);
      await openTicketProperties(tester);
      await tester.ensureVisible(find.byKey(const ValueKey('ticket-row-mode')));
      await tester.pump();
      expect(find.text('Plan'), findsOneWidget, reason: 'no approval in the timeline yet');
    });

    testWidgets('Approve, accept edits is approve_edits', (tester) async {
      final repo = await open(tester, planned(), messages: plan, plan: true);
      await tester.tap(find.byKey(const ValueKey('ticket-plan-approve-edits')));
      await settleBoard(tester);
      expect(paramsOf(writesOf(repo).single)['decision'], 'approve_edits');
    });

    testWidgets('Request changes needs feedback (B2), then ticket.revise_plan', (tester) async {
      final repo = await open(tester, planned(), messages: plan, plan: true);
      expect(tester.widget<OutlinedButton>(find.byKey(const ValueKey('ticket-plan-revise'))).onPressed, isNull);
      await tester.enterText(find.byKey(const ValueKey('ticket-plan-feedback')), 'Add a migration test.');
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('ticket-plan-revise')));
      await settleBoard(tester);
      final revise = writesOf(repo).single;
      expect(revise['method'], 'ticket.revise_plan');
      expect(paramsOf(revise), {'ticket_id': 'k1', 'feedback': 'Add a migration test.'});
    });

    testWidgets('Reject asks, then puts it back in Backlog (ticket.cancel)', (tester) async {
      final repo = await open(tester, planned(), messages: plan, plan: true);
      await tester.tap(find.byKey(const ValueKey('ticket-plan-reject')));
      await settleBoard(tester);
      expect(find.text('This rejects the plan and stops the work.'), findsOneWidget);
      expect(find.text('Reject the plan of #42?'), findsOneWidget);
      expect(find.widgetWithText(TextButton, 'Back to Backlog'), findsNothing, reason: 'the button says what it does here');
      await tester.tap(find.descendant(of: find.byType(AlertDialog), matching: find.widgetWithText(TextButton, 'Reject')));
      await settleBoard(tester);
      expect(writesOf(repo).single['method'], 'ticket.cancel');
    });

    testWidgets('a plan decided elsewhere says so and offers nothing', (tester) async {
      await open(tester, detailOf(ticketOf(column: ColumnKind.doing, rootTaskId: 'root'), root: root), messages: plan, plan: true);
      expect(find.byKey(const ValueKey('plan-handled')), findsOneWidget);
      expect(tester.widget<FilledButton>(find.byKey(const ValueKey('ticket-plan-approve'))).onPressed, isNull);
    });
  });
}
