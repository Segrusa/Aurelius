import 'dart:async';

import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/discussion.dart';
import 'package:aurelius_mobile/data/drafts.dart';
import 'package:aurelius_mobile/data/tickets.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/board_bloc.dart';
import 'package:aurelius_mobile/features/board/board_page.dart';
import 'package:aurelius_mobile/features/board/new_ticket_page.dart';
import 'package:aurelius_mobile/features/board/ticket_page.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:aurelius_mobile/features/inbox/inbox_bloc.dart';
import 'package:aurelius_mobile/features/onboarding/onboarding.dart';
import 'package:aurelius_mobile/features/runtime/runtime_bloc.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:intl/intl.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/fake_daemon.dart';
import 'support/tickets.dart';

// AUR-MU-03-M3: the mobile fixes of R3 and QA3 (block B). Neutral people
// only: Alex (testUser on the board harness) and Robin, test@example.com.

const _robin = UserView(id: 'u-2', username: 'Robin', createdAtMs: 1);
const _alexPassword = 'alex long password';
const _robinPassword = 'robin long password';

const _missing = ClaudeLoginView(userId: 'u-1', status: ClaudeLoginStatus.missing, background: false);
const _expired = ClaudeLoginView(userId: 'u-1', status: ClaudeLoginStatus.expired, background: false);

LinkState _linkWith(ClaudeLoginView? login) => LinkState(status: LinkStatus.online, capabilities: onlineLink.capabilities, boardMembersExist: true, user: testUser, claudeLogin: login);

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));
  setUp(DraftStore.instance.clear);

  group('R3 M1: a draft outlives signed_out, for the same board member only', () {
    late FakeDaemon daemon;
    late AureliusConnection connection;

    setUp(() async {
      daemon = FakeDaemon()
        ..addUser('u-1', 'alex', _alexPassword)
        ..addUser('u-2', 'robin', _robinPassword);
      connection = AureliusConnection(host: testHost, store: await storeWithSecret(), api: FakeApi(), connect: daemon.connect, deviceName: 'Test phone');
      connection.start();
      await until(() => connection.state.isOnline);
    });

    tearDown(() => connection.dispose());

    test('per host, board member and slot; an empty text forgets it', () async {
      final repository = AureliusRepository(connection);
      await connection.signIn('alex', _alexPassword);
      repository.keepDraft('ticket:k1', 'Half a thought');
      expect(repository.draft('ticket:k1'), 'Half a thought');
      expect(repository.draft('ticket:k2'), isNull, reason: 'another ticket');
      expect(DraftStore.instance.read('host-2', 'u-1', 'ticket:k1'), isNull, reason: 'another host');

      await connection.signOut();
      expect(repository.draft('ticket:k1'), isNull, reason: 'nobody signed in: nothing shown');
      await connection.signIn('robin', _robinPassword);
      expect(repository.draft('ticket:k1'), isNull, reason: 'Robin never sees Alex\'s draft');
      repository.keepDraft('ticket:k1', 'Robin writes too');
      await connection.signOut();

      await connection.signIn('alex', _alexPassword);
      expect(repository.draft('ticket:k1'), 'Half a thought', reason: 'back for Alex');
      repository.keepDraft('ticket:k1', '');
      expect(repository.draft('ticket:k1'), isNull, reason: 'sent: forgotten');
    });
  });

  group('R3 M1: the composers keep their draft across a sign-out', () {
    late MockRepository repo;
    late MockConnectionBloc link;
    late StreamController<LinkState> links;
    late ValueNotifier<BoardMemberStep> step;
    var current = onlineLink;

    const phase2 = LinkState(
      status: LinkStatus.online,
      capabilities: ['read', 'chat', 'task_control', 'approve'],
      features: [ticketDiscussionFeature],
      boardMembersExist: true,
      user: testUser,
      claudeLogin: testLogin,
    );
    const signedOut = LinkState(status: LinkStatus.online, features: [ticketDiscussionFeature], boardMembersExist: true, signedOut: SignOutReason.expired);

    void setLink(LinkState state) {
      current = state;
      when(() => link.state).thenReturn(state);
      when(() => repo.linkState).thenReturn(state);
      links.add(state);
      step.value = state.boardMemberStep;
    }

    setUp(() {
      final ticket = TicketView.fromJson({
        ...ticketOf(column: ColumnKind.doing, ownerEmployeeId: 'forge', ownerName: 'Forge', rootTaskId: 'root').toJson(),
        'conversation_id': 'disc-1',
      });
      repo = boardRepository(detail: TicketDetail(asOfSeq: 1, ticket: ticket, tasks: const []), messages: snapshotOf('disc-1'));
      when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.appends).thenAnswer((_) => const Stream.empty());
      when(() => repo.loadChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.load()).thenAnswer((_) async => loadOf());
      when(() => repo.boardMembers()).thenAnswer((_) async => const [testUser, _robin]);
      // The repository's own rule, over the real store: per host and the signed-in board member.
      when(() => repo.draft(any())).thenAnswer((i) {
        final me = current.user?.id;
        return me == null ? null : DraftStore.instance.read('h', me, i.positionalArguments.single as String);
      });
      when(() => repo.keepDraft(any(), any())).thenAnswer((i) {
        final me = current.user?.id;
        if (me != null) DraftStore.instance.write('h', me, i.positionalArguments[0] as String, i.positionalArguments[1] as String);
      });
      link = MockConnectionBloc();
      links = StreamController<LinkState>.broadcast();
      step = ValueNotifier(phase2.boardMemberStep);
      whenListen(link, links.stream, initialState: phase2);
    });

    tearDown(() {
      links.close();
      step.dispose();
    });

    /// The app's own gate: signed out, only /onboarding; signed in, back home.
    Future<void> pumpApp(WidgetTester tester) async {
      tester.view.physicalSize = const Size(390, 844) * 3;
      tester.view.devicePixelRatio = 3;
      addTearDown(tester.view.reset);
      final company = MockCompanyBloc();
      whenListen(company, const Stream<CompanyState>.empty(), initialState: companyWith());
      final inbox = MockInboxBloc();
      whenListen(inbox, const Stream<InboxState>.empty(), initialState: const InboxState(loading: false));
      final runtime = MockRuntimeBloc();
      whenListen(runtime, const Stream<RuntimeState>.empty(), initialState: const RuntimeState());
      final router = GoRouter(
        initialLocation: '/chat',
        refreshListenable: step,
        redirect: (_, state) => boardMemberRedirect(step.value, state.matchedLocation),
        routes: [
          GoRoute(path: '/chat', builder: (_, _) => const TicketPage(ticketId: 'k1')),
          GoRoute(path: '/onboarding', builder: (_, _) => const Scaffold(body: Text('You were signed out on this device.'))),
        ],
      );
      addTearDown(router.dispose);
      setLink(phase2);
      await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
        value: repo,
        child: MultiBlocProvider(
          providers: [
            BlocProvider<CompanyBloc>.value(value: company),
            BlocProvider<ConnectionBloc>.value(value: link),
            BlocProvider<InboxBloc>.value(value: inbox),
            BlocProvider<RuntimeBloc>.value(value: runtime),
          ],
          child: MaterialApp.router(theme: aureliusTheme(), routerConfig: router),
        ),
      ));
      await settleBoard(tester);
    }

    testWidgets('typed in the discussion → signed_out → signed in again: the text is still there', (tester) async {
      await pumpApp(tester);
      await tester.enterText(find.byType(TextField).last, 'Keep the hash parameters configurable');
      await tester.pump();

      setLink(signedOut);
      await settleBoard(tester);
      expect(find.text('You were signed out on this device.'), findsOneWidget);
      expect(find.byType(TicketPage), findsNothing, reason: 'the page and its composer are gone');

      setLink(phase2);
      await settleBoard(tester);
      expect(find.byType(TicketPage), findsOneWidget);
      expect(find.text('Keep the hash parameters configurable'), findsOneWidget);

      // Sent: the draft is forgotten.
      await tester.tap(find.byKey(const Key('composer-send')));
      await settleBoard(tester);
      expect(writesOf(repo).single['method'], 'ticket.post');
      expect(DraftStore.instance.read('h', 'u-1', 'ticket:k1'), isNull);
    });

    testWidgets('another board member signing in on this phone does not get it', (tester) async {
      await pumpApp(tester);
      await tester.enterText(find.byType(TextField).last, 'Only for Alex');
      await tester.pump();
      setLink(signedOut);
      await settleBoard(tester);
      setLink(LinkState(status: LinkStatus.online, capabilities: phase2.capabilities, features: phase2.features, boardMembersExist: true, user: _robin, claudeLogin: testLogin));
      await settleBoard(tester);
      expect(find.byType(TicketPage), findsOneWidget);
      expect(find.text('Only for Alex'), findsNothing);
      expect(DraftStore.instance.read('h', 'u-1', 'ticket:k1'), 'Only for Alex', reason: 'kept for Alex');
    });
  });

  group('R3 M1: the plan feedback (a server without discussions) keeps its draft', () {
    testWidgets('restored from the store, kept while typed, forgotten once sent', (tester) async {
      final ticket = ticketOf(column: ColumnKind.planReview, ownerEmployeeId: 'forge', ownerName: 'Forge', rootTaskId: 'root', run: const TaskRun(mode: AgentMode.plan));
      final root = taskOf(id: 'root', conversationId: 'conv-root', ticketId: 'k1');
      final repo = boardRepository(
        detail: TicketDetail(asOfSeq: 1, ticket: ticket, rootTask: root, tasks: [root]),
        messages: snapshotOf('conv-root', messages: [messageOf('p1', MessageRole.assistant, 'The plan.')]),
      );
      final kept = <String, String>{'plan-feedback:k1': 'Add a migration test.'};
      when(() => repo.draft(any())).thenAnswer((i) => kept[i.positionalArguments.single as String]);
      when(() => repo.keepDraft(any(), any())).thenAnswer((i) => kept[i.positionalArguments[0] as String] = i.positionalArguments[1] as String);
      await pumpBoardPage(tester, repo, const TicketPlanPage(ticketId: 'k1'));
      expect(find.text('Add a migration test.'), findsOneWidget);
      await tester.enterText(find.byKey(const ValueKey('ticket-plan-feedback')), 'Add a migration test, and a rollback.');
      await tester.pump();
      expect(kept['plan-feedback:k1'], 'Add a migration test, and a rollback.');
      await tester.tap(find.byKey(const ValueKey('ticket-plan-revise')));
      await settleBoard(tester);
      expect(writesOf(repo).single['method'], 'ticket.revise_plan');
      expect(kept['plan-feedback:k1'], '', reason: 'sent: the draft is done with');
    });
  });

  group('QA3 M2: the Board follows Create after a switch of board member on the same phone', () {
    test('Alex signs out, Robin signs in, creates a ticket: the board reads it again', () async {
      final daemon = FakeDaemon()
        ..addUser('u-1', 'alex', _alexPassword)
        ..addUser('u-2', 'robin', _robinPassword);
      final connection = AureliusConnection(host: testHost, store: await storeWithSecret(), api: FakeApi(), connect: daemon.connect, deviceName: 'Test phone');
      addTearDown(connection.dispose);
      connection.start();
      await until(() => connection.state.isOnline);
      final repository = AureliusRepository(connection);
      await connection.signIn('alex', _alexPassword);
      await connection.signOut();
      await connection.signIn('robin', _robinPassword);

      // The Board of the shell that comes after the sign-in.
      final bloc = BoardBloc(repository, timeout: const Duration(seconds: 2));
      addTearDown(bloc.close);
      await until(() => bloc.state.board != null);
      expect(bloc.state.tickets, isEmpty);

      await repository.write(ticketCreate(title: 'Columns', projectId: 'p1'));
      await until(() => bloc.state.tickets.length == 1);
      expect(bloc.state.tickets.single.createdByUsername, 'robin');

      await repository.write(ticketCreate(title: 'Rows', projectId: 'p1', start: true));
      await until(() => bloc.state.tickets.length == 2);
    });
  });

  group('the Board and the ticket page', () {
    Widget boardPage(MockRepository repo) => BlocProvider(create: (_) => BoardBloc(repo), child: const Scaffold(body: BoardPage()));

    testWidgets('R3 m4: an expired own login says Reconnect Claude on the card', (tester) async {
      final repo = boardRepository(board: boardOf([ticketOf(column: ColumnKind.doing, needs: TicketNeed.login)]));
      await pumpBoardPage(tester, repo, boardPage(repo), link: _linkWith(_expired));
      expect(find.text('Reconnect Claude'), findsOneWidget);
      expect(find.text('Connect Claude'), findsNothing);
    });

    testWidgets('R3 m4: a missing own login says Connect Claude', (tester) async {
      final repo = boardRepository(board: boardOf([ticketOf(column: ColumnKind.doing, needs: TicketNeed.login)]));
      await pumpBoardPage(tester, repo, boardPage(repo), link: _linkWith(_missing));
      expect(find.text('Connect Claude'), findsOneWidget);
    });

    testWidgets('QA3 M-m4: the limit wait on a card takes the row\'s width and wraps to a second line, so the time stays readable', (tester) async {
      final until = DateTime.now().add(const Duration(minutes: 30));
      final root = taskOf(id: 'root', status: TaskStatus.waiting, waitingReason: 'quota', waitingUntilMs: until.millisecondsSinceEpoch, ticketId: 'k1');
      final sub = taskOf(id: 'sub', status: TaskStatus.queued, ticketId: 'k1', parentTaskId: 'root');
      final ticket = ticketOf(column: ColumnKind.doing, creator: _robin, rootTaskId: 'root', rootTaskStatus: TaskStatus.waiting);
      final repo = boardRepository(board: boardOf([ticket]));
      await pumpBoardPage(tester, repo, boardPage(repo), company: companyWith(tasks: [root, sub]), size: const Size(420, 844));
      final key = find.byKey(const ValueKey('card-line-k1'));
      // The whole text, with the time (QA3 saw "Waiting for alex's limit unti…" at 420 px).
      expect(tester.widget<Text>(key).data, limitWaitOf("Robin's", until.millisecondsSinceEpoch));
      expect(tester.widget<Text>(key).data, endsWith(DateFormat.Hm().format(until)));
      expect(tester.widget<Text>(key).maxLines, 2, reason: 'wraps over two lines instead of cutting off');
      // The line has the row to itself, beside the mark and the delegations (no Spacer taking half).
      final row = tester.getSize(find.ancestor(of: key, matching: find.byType(Row)).first).width;
      final delegations = tester.getSize(find.text('⑂ 1 task')).width;
      expect(tester.getSize(key).width, greaterThan(row - delegations - 40));
      final paragraph = tester.renderObject<RenderParagraph>(find.descendant(of: key, matching: find.byType(RichText)));
      expect(paragraph.size.height, greaterThan(20), reason: 'two lines tall in the test font');
    });

    testWidgets('R3 m5: Start while the own login was not read yet says it waits (P-36)', (tester) async {
      final repo = boardRepository(detail: TicketDetail(asOfSeq: 1, ticket: ticketOf(number: 7), tasks: const []));
      await pumpBoardPage(tester, repo, const TicketPage(ticketId: 'k1'), link: _linkWith(null));
      await tester.ensureVisible(find.byKey(const ValueKey('bar-start')));
      await tester.tap(find.byKey(const ValueKey('bar-start')));
      await settleBoard(tester);
      expect(find.text('#7 started. It waits until you connect your Claude login.'), findsOneWidget);
    });

    testWidgets('QA3 M-m2: Create and start without a login gives the P-36 toast', (tester) async {
      final repo = boardRepository();
      when(() => repo.linkState).thenReturn(_linkWith(_missing));
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((_) async => receiptOf(const CommandResultTicketCreated(number: 52, ticketId: 'k52')));
      await pumpBoardPage(tester, repo, const NewTicketPage(projectId: 'p1'), link: _linkWith(_missing));
      await tester.enterText(find.byKey(const ValueKey('new-ticket-title')), 'Columns');
      await tester.ensureVisible(find.byKey(const ValueKey('new-ticket-create-start')));
      await tester.tap(find.byKey(const ValueKey('new-ticket-create-start')));
      await settleBoard(tester);
      expect(find.text('#52 created and started. It waits until you connect your Claude login.'), findsOneWidget);
    });

    testWidgets('QA3 M-m2: with a connected login Create and start says only that', (tester) async {
      final repo = boardRepository();
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((_) async => receiptOf(const CommandResultTicketCreated(number: 52, ticketId: 'k52')));
      await pumpBoardPage(tester, repo, const NewTicketPage(projectId: 'p1'));
      await tester.enterText(find.byKey(const ValueKey('new-ticket-title')), 'Columns');
      await tester.ensureVisible(find.byKey(const ValueKey('new-ticket-create-start')));
      await tester.tap(find.byKey(const ValueKey('new-ticket-create-start')));
      await settleBoard(tester);
      expect(find.text('#52 created and started.'), findsOneWidget);
    });

    testWidgets('QA3 M-m3: after Run on my login the ticket page reads the ticket again, without waiting for an event', (tester) async {
      final waiting = ticketOf(id: 'k1', number: 42, column: ColumnKind.doing, creator: _robin, needs: TicketNeed.login);
      final mine = ticketOf(id: 'k1', number: 42, column: ColumnKind.doing, creator: _robin, login: testUser, version: 4);
      final repo = boardRepository(detail: TicketDetail(asOfSeq: 1, ticket: waiting, tasks: const []));
      var used = false;
      when(() => repo.ticket('k1')).thenAnswer((_) async => TicketDetail(asOfSeq: used ? 2 : 1, ticket: used ? mine : waiting, tasks: const []));
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry'))).thenAnswer((_) async {
        used = true;
        return receiptOf(const CommandResultTicketChanged(column: ColumnKind.doing, ticketId: 'k1', version: 4));
      });
      await pumpBoardPage(tester, repo, const TicketPage(ticketId: 'k1'));
      expect(find.text("⏸ Waiting for Robin's Claude login"), findsOneWidget);
      await tester.ensureVisible(find.byKey(const ValueKey('bar-use-my-login')));
      await tester.tap(find.byKey(const ValueKey('bar-use-my-login')));
      await settleBoard(tester);
      await tester.tap(find.text('Run on my login').last);
      await settleBoard(tester);
      expect(find.text("⏸ Waiting for Robin's Claude login"), findsNothing);
      expect(find.byKey(const ValueKey('bar-use-my-login')), findsNothing);
      await openTicketProperties(tester);
      expect(find.text('Your Claude login'), findsOneWidget, reason: 'Runs on: the own login now');
    });

    testWidgets('R3 m6: Created by: Anyone / Me / a board member', (tester) async {
      final repo = boardRepository(board: boardOf([
        ticketOf(id: 'k1', number: 1, title: 'Mine'),
        ticketOf(id: 'k2', number: 2, title: 'Robin\'s', creator: _robin),
      ]));
      when(() => repo.boardMembers()).thenAnswer((_) async => const [testUser, _robin]);
      // Wide enough for the three pills in the test font (each glyph 1 em); still the phone layout.
      await pumpBoardPage(tester, repo, boardPage(repo), size: const Size(660, 900));
      expect(find.text('2 tickets'), findsOneWidget);
      expect(find.text('Created by: Anyone'), findsOneWidget);

      await tester.ensureVisible(find.byKey(const ValueKey('board-filter-creator')));
      await tester.tap(find.byKey(const ValueKey('board-filter-creator')));
      await settleBoard(tester);
      expect(find.byKey(const ValueKey('board-creator-me')), findsOneWidget);
      expect(find.byKey(const ValueKey('board-creator-u-1')), findsNothing, reason: 'the own one is "Me"');
      await tester.tap(find.byKey(const ValueKey('board-creator-me')));
      await settleBoard(tester);
      expect(find.text('Created by: Me'), findsOneWidget);
      expect(find.text('1 ticket'), findsOneWidget);

      await tester.ensureVisible(find.byKey(const ValueKey('board-filter-creator')));
      await tester.tap(find.byKey(const ValueKey('board-filter-creator')));
      await settleBoard(tester);
      await tester.tap(find.byKey(const ValueKey('board-creator-u-2')));
      await settleBoard(tester);
      expect(find.text('Created by: Robin'), findsOneWidget);
      expect(find.text('1 ticket'), findsOneWidget);

      await tester.ensureVisible(find.byKey(const ValueKey('board-filter-creator')));
      await tester.tap(find.byKey(const ValueKey('board-filter-creator')));
      await settleBoard(tester);
      await tester.tap(find.byKey(const ValueKey('board-creator-anyone')));
      await settleBoard(tester);
      expect(find.text('2 tickets'), findsOneWidget);
    });
  });
}
