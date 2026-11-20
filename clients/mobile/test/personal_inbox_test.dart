import 'dart:async';

import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/inbox/inbox_bloc.dart';
import 'package:aurelius_mobile/features/inbox/inbox_page.dart';
import 'package:aurelius_mobile/features/inbox/inbox_rows.dart';
import 'package:aurelius_mobile/features/inbox/inbox_sections.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:mocktail/mocktail.dart';

// A3 on the phone (freeze §4): the rows, where they open, the ◆, `+n` and a
// server without `personal_inbox`.

class _MockRepository extends Mock implements AureliusRepository {}

class _MockInboxBloc extends MockBloc<InboxEvent, InboxState> implements InboxBloc {}

class _MockCompanyBloc extends MockBloc<CompanyEvent, CompanyState> implements CompanyBloc {}

const _host = Host(hostId: '01a19bd5-6957-73a6-a074-d98f1c80f0b2', name: 'Segrusa', url: 'http://x', clientId: 'c', environment: 'dev', pairedAtMs: 0);

NotificationView row(
  String id, {
  String? reason = 'mention',
  String title = 'Forge mentioned you',
  String subject = 'Login per board member',
  String? ticket = 'k42',
  int? number = 42,
  String? conversation,
  String? message,
  int? merged,
  bool? needsYou,
  bool read = false,
  String severity = 'info',
  int at = 1000,
}) =>
    NotificationView(id: id, severity: severity, title: title, subject: subject, createdAtMs: at, read: read, targetType: 'ticket.review', reason: reason, ticketId: ticket, ticketNumber: number, conversationId: conversation, messageId: message, mergedCount: merged, needsYou: needsYou);

NotificationView result(String id) => NotificationView(id: id, severity: 'info', title: 'Result ready', subject: 'Subject $id', createdAtMs: 0, read: false, targetType: 'task.done');

LinkState _link({bool personal = true}) => LinkState(
      status: LinkStatus.online,
      capabilities: const ['read', 'chat', 'task_control', 'approve'],
      features: ['ticket_follow', 'message_locate', if (personal) 'personal_inbox'],
      boardMembersExist: true,
      user: const UserView(id: 'u-1', username: 'ada', createdAtMs: 0),
    );

void main() {
  setUpAll(() {
    registerFallbackValue(<String, dynamic>{});
    registerFallbackValue(const InboxRefreshed() as InboxEvent);
  });

  group('a row says', () {
    test('the place as #42 and title, or # channel', () {
      expect(row('a').place, '#42 Login per board member');
      expect(row('a', ticket: null, number: null, conversation: 'c1', subject: 'general').place, '# general');
    });

    test('the place does not repeat a number the server subject already has', () {
      expect(row('a', subject: '#2 Check the export', number: 2).place, '#2 Check the export');
      expect(row('a', subject: '#2', number: 2).place, '#2');
      expect(row('a', subject: '#22 Other', number: 2).place, '#2 #22 Other', reason: 'only the same number counts');
    });

    test('review follows needs_you, so it follows a handover', () {
      expect(row('a', reason: 'review', title: 'Marie: stale', needsYou: true).headline, 'Ready for your review');
      expect(row('a', reason: 'review', title: 'Marie: stale').headline, 'Ready for review');
      expect(row('a', reason: 'plan_review', needsYou: true).headline, 'Plan ready for your review');
      expect(row('a', reason: 'plan_review').headline, 'Plan ready for review');
    });

    test('failed and login have their words; who did it comes from the server', () {
      expect(row('a', reason: 'failed').headline, 'The main task failed');
      expect(row('a', reason: 'login').headline, 'Waiting for your Claude login');
      expect(row('a', reason: 'responsible', title: 'Marie made you responsible').headline, 'Marie made you responsible');
      expect(row('a', reason: 'sent_back', title: 'Marie sent it back to Doing').headline, 'Marie sent it back to Doing');
    });

    test('an unknown reason is a plain row with the server\'s sentence', () {
      final n = row('a', reason: 'from_the_future', title: 'Something new happened');
      expect(n.isPersonal, isTrue);
      expect(n.headline, 'Something new happened');
      expect(n.waitsForMe, isFalse);
      expect(inboxPlace(reason: n.reason, ticketId: n.ticketId), '/ticket/k42');
    });

    test('a screen reader hears all of it in one sentence', () {
      expect(row('a', reason: 'review', needsYou: true, merged: 2).spoken, '#42 Login per board member, Ready for your review, needs you, 2 more updates, unread');
    });

    test('marking read keeps every field of the row', () {
      final read = readRow(row('a', reason: 'review', needsYou: true, merged: 2, conversation: 'c', message: 'm'));
      expect((read.read, read.reason, read.needsYou, read.mergedCount, read.ticketId, read.ticketNumber, read.conversationId, read.messageId), (true, 'review', true, 2, 'k42', 42, 'c', 'm'));
    });
  });

  group('inboxPlace', () {
    test('opens each reason at its place', () {
      expect(inboxPlace(reason: 'responsible', ticketId: 't'), '/ticket/t');
      expect(inboxPlace(reason: 'review', ticketId: 't'), '/ticket/t');
      expect(inboxPlace(reason: 'failed', ticketId: 't'), '/ticket/t');
      expect(inboxPlace(reason: 'login', ticketId: 't'), '/ticket/t');
      expect(inboxPlace(reason: 'plan_review', ticketId: 't'), '/ticket/t/plan');
      expect(inboxPlace(reason: 'sent_back', ticketId: 't'), '/ticket/t?tab=discussion');
      expect(inboxPlace(reason: 'mention', ticketId: 't', messageId: 'm'), '/ticket/t?tab=discussion&message=m');
    });

    test('outside a ticket (chat is gone) it opens the Inbox, never a channel', () {
      expect(inboxPlace(reason: 'mention', conversationId: 'c', messageId: 'm'), '/inbox');
      expect(inboxPlace(reason: 'mention', conversationId: 'c'), '/inbox');
    });

    test('naming neither opens nothing', () => expect(inboxPlace(reason: 'mention'), isNull));
  });

  group('the sections', () {
    test('split the rows by the ◆ and keep results in Done', () {
      final s = InboxSections.of(InboxState(personalOn: true, notifications: [row('w', reason: 'review', needsYou: true), row('u'), result('r')]), const CompanyState());
      expect(s.rowsNeedingYou.map((n) => n.id), ['w']);
      expect(s.updates.map((n) => n.id), ['u']);
      expect(s.results.map((n) => n.id), ['r']);
      expect(s.needsYou, 1);
    });

    test('a server without personal_inbox has no rows', () {
      final s = InboxSections.of(InboxState(notifications: [result('r')]), const CompanyState());
      expect(s.rowsNeedingYou, isEmpty);
      expect(s.updates, isEmpty);
      expect(s.results.map((n) => n.id), ['r']);
    });

    test('the badge counts unread rows and every ◆, not a read update', () {
      final state = InboxState(personalOn: true, notifications: [row('w', needsYou: true, read: true), row('u'), row('seen', read: true), result('r')]);
      expect(state.badge, 2);
    });
  });

  group('InboxBloc', () {
    late _MockRepository repo;
    setUp(() {
      repo = _MockRepository();
      when(() => repo.online).thenAnswer((_) => const Stream.empty());
      when(() => repo.companyChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.linkState).thenReturn(_link());
      when(() => repo.approvals()).thenAnswer((_) async => []);
      when(() => repo.proposals()).thenAnswer((_) async => []);
      when(() => repo.guidelineProposalList(status: any(named: 'status'))).thenAnswer((_) async => []);
      when(() => repo.markNotificationsRead()).thenAnswer((_) async {});
      when(() => repo.markNotificationRead(any())).thenAnswer((_) async {});
      when(() => repo.dismissNotification(any())).thenAnswer((_) async {});
      when(() => repo.dismissAllNotifications()).thenAnswer((_) async => 0);
    });

    Future<InboxBloc> loaded(List<NotificationView> rows) async {
      when(() => repo.notifications()).thenAnswer((_) async => NotificationsSnapshot(notifications: rows, unread: rows.where((n) => !n.read).length));
      final bloc = InboxBloc(repo);
      addTearDown(bloc.close);
      await bloc.stream.firstWhere((s) => !s.loading);
      return bloc;
    }

    test('seeing the list reads nothing while a row is unread', () async {
      final bloc = await loaded([row('u'), result('r')]);
      bloc.add(const NotificationsMarkedRead());
      await pumpEventQueue();
      verifyNever(() => repo.markNotificationsRead());
      expect(bloc.state.unread, 2);
    });

    test('with no unread row, seeing the list reads Done as before', () async {
      final bloc = await loaded([row('u', read: true), result('r')]);
      bloc.add(const NotificationsMarkedRead());
      await pumpEventQueue();
      verify(() => repo.markNotificationsRead()).called(1);
      expect(bloc.state.notifications.where((n) => n.id == 'u').single.reason, 'mention', reason: 'mark-all keeps what the row is about');
    });

    test('opening a row reads that row only, and it stays read over a refresh', () async {
      final bloc = await loaded([row('a'), row('b', ticket: 'k43', number: 43)]);
      bloc.add(InboxRowOpened(bloc.state.notifications.first));
      await pumpEventQueue();
      expect(bloc.state.notifications.map((n) => n.read), [true, false]);
      expect(bloc.state.unread, 1);
      bloc.add(const InboxRefreshed());
      await bloc.stream.first;
      expect(bloc.state.notifications.map((n) => n.read), [true, false]);
      expect(bloc.state.unread, 1);
      verifyNever(() => repo.markNotificationsRead());
    });

    test('opening a row tells the server once, and not again for the same row', () async {
      final bloc = await loaded([row('a'), row('b', ticket: 'k43', number: 43)]);
      final first = bloc.state.notifications.first;
      bloc.add(InboxRowOpened(first));
      bloc.add(InboxRowOpened(first));
      await pumpEventQueue();
      bloc.add(InboxRowOpened(bloc.state.notifications.first));
      await pumpEventQueue();
      verify(() => repo.markNotificationRead('a')).called(1);
      verifyNever(() => repo.markNotificationRead('b'));
    });

    test('an already read row sends no write when it is opened', () async {
      final bloc = await loaded([row('a', read: true)]);
      bloc.add(InboxRowOpened(bloc.state.notifications.first));
      await pumpEventQueue();
      verifyNever(() => repo.markNotificationRead(any()));
    });

    test('a write that fails leaves the row read here and is sent again at the next open', () async {
      when(() => repo.markNotificationRead(any())).thenThrow(const ProtocolException(ErrorCode.internal, 'no'));
      final bloc = await loaded([row('a')]);
      bloc.add(InboxRowOpened(bloc.state.notifications.first));
      await pumpEventQueue();
      expect(bloc.state.notifications.single.read, isTrue);
      expect(bloc.state.notice, isNull);
      bloc.add(InboxRowOpened(row('a')));
      await pumpEventQueue();
      verify(() => repo.markNotificationRead('a')).called(2);
    });

    test('a new event on an opened row makes it unread again', () async {
      final bloc = await loaded([row('a', at: 1000)]);
      bloc.add(InboxRowOpened(bloc.state.notifications.first));
      await pumpEventQueue();
      when(() => repo.notifications()).thenAnswer((_) async => NotificationsSnapshot(notifications: [row('a', at: 2000, merged: 1)], unread: 1));
      bloc.add(const InboxRefreshed());
      await bloc.stream.first;
      expect(bloc.state.notifications.single.read, isFalse);
      expect(bloc.state.unread, 1);
    });

    test('Clear all clears the results only, one by one, never the rows', () async {
      final bloc = await loaded([row('u'), row('w', needsYou: true), result('r1'), result('r2')]);
      bloc.add(const NotificationsCleared());
      await pumpEventQueue();
      verify(() => repo.dismissNotification('r1')).called(1);
      verify(() => repo.dismissNotification('r2')).called(1);
      verifyNever(() => repo.dismissAllNotifications());
      expect(bloc.state.notifications.map((n) => n.id), ['u', 'w']);
      expect(bloc.state.unread, 2);
    });

    test('opening a task leaves its rows; opening the row reads them', () async {
      final mainRow = NotificationView(id: 'm', severity: 'warning', title: 't', subject: 's', createdAtMs: 0, read: false, targetType: 'x', taskId: 't1', reason: 'failed', ticketId: 'k');
      final bloc = await loaded([mainRow]);
      bloc.add(const TaskOpened('t1'));
      await pumpEventQueue();
      verifyNever(() => repo.dismissNotification(any()));
      expect(bloc.state.notifications, hasLength(1));
    });

    test('a read that fails says so, it is no endless wait and no empty Inbox', () async {
      when(() => repo.notifications()).thenThrow(const ProtocolException(ErrorCode.internal, 'no'));
      final bloc = InboxBloc(repo);
      addTearDown(bloc.close);
      final s = await bloc.stream.firstWhere((s) => !s.loading);
      expect(s.loadFailed, isTrue);
    });

    test('a mention is a chat event: it refreshes the rows', () async {
      final chat = StreamController<Event>.broadcast();
      addTearDown(chat.close);
      when(() => repo.chatChanges).thenAnswer((_) => chat.stream);
      final bloc = await loaded([row('a')]);
      when(() => repo.notifications()).thenAnswer((_) async => NotificationsSnapshot(notifications: [row('a'), row('b', ticket: 'k43')], unread: 2));
      chat.add(const Event(aggregateId: 'x', atMs: 0, kind: EventKind.messageUserCommitted, seq: 1));
      await bloc.stream.firstWhere((s) => s.notifications.length == 2);
    });

    group('on a server without personal_inbox', () {
      setUp(() => when(() => repo.linkState).thenReturn(_link(personal: false)));

      test('it works as now: chat events are not read, the list reads all, Clear all is one call', () async {
        final bloc = await loaded([result('r1'), result('r2')]);
        expect(bloc.state.personalOn, isFalse);
        verifyNever(() => repo.chatChanges);
        bloc.add(const NotificationsMarkedRead());
        await pumpEventQueue();
        verify(() => repo.markNotificationsRead()).called(1);
        bloc.add(const NotificationsCleared());
        await pumpEventQueue();
        verify(() => repo.dismissAllNotifications()).called(1);
        verifyNever(() => repo.dismissNotification(any()));
      });

      test('opening a row sends no per-row write', () async {
        final bloc = await loaded([row('a')]);
        bloc.add(InboxRowOpened(bloc.state.notifications.first));
        await pumpEventQueue();
        verifyNever(() => repo.markNotificationRead(any()));
        expect(bloc.state.notifications.single.read, isTrue);
      });

      test('a failed read keeps the old notice, no new screen', () async {
        when(() => repo.notifications()).thenThrow(const ProtocolException(ErrorCode.internal, 'no'));
        final bloc = InboxBloc(repo);
        addTearDown(bloc.close);
        final s = await bloc.stream.firstWhere((s) => !s.loading);
        expect(s.loadFailed, isFalse);
        expect(s.notice, 'no');
      });
    });
  });

  group('the Inbox page', () {
    Future<(_MockRepository, _MockInboxBloc, GoRouter)> show(WidgetTester tester, InboxState inbox, {LinkState? link}) async {
      tester.view.physicalSize = const Size(1200, 3600);
      addTearDown(tester.view.resetPhysicalSize);
      final repo = _MockRepository();
      when(() => repo.host).thenReturn(_host);
      when(() => repo.linkState).thenReturn(link ?? _link());
      final bloc = _MockInboxBloc();
      whenListen(bloc, const Stream<InboxState>.empty(), initialState: inbox);
      final company = _MockCompanyBloc();
      whenListen(company, const Stream<CompanyState>.empty(), initialState: const CompanyState(loading: false));
      final router = GoRouter(routes: [
        GoRoute(path: '/', builder: (_, _) => const InboxPage()),
        GoRoute(path: '/ticket/:id', builder: (_, s) => Scaffold(body: Text('ticket ${s.pathParameters['id']} tab ${s.uri.queryParameters['tab']}'))),
        GoRoute(path: '/ticket/:id/plan', builder: (_, s) => Scaffold(body: Text('plan ${s.pathParameters['id']}'))),
        GoRoute(path: '/inbox', builder: (_, _) => const Scaffold(body: Text('inbox'))),
      ]);
      addTearDown(router.dispose);
      await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
        value: repo,
        child: MultiBlocProvider(
          providers: [BlocProvider<InboxBloc>.value(value: bloc), BlocProvider<CompanyBloc>.value(value: company)],
          child: MaterialApp.router(theme: aureliusTheme(), routerConfig: router),
        ),
      ));
      await tester.pump(const Duration(milliseconds: 100));
      return (repo, bloc, router);
    }

    testWidgets('a ◆ row sits in Needs you, the others in Updates and done', (tester) async {
      await show(tester, InboxState(loading: false, personalOn: true, notifications: [row('w', reason: 'review', needsYou: true), row('u', ticket: 'k43', number: 43)]));
      expect(find.byKey(const ValueKey('inbox-row-diamond-w')), findsOneWidget);
      expect(find.byKey(const ValueKey('inbox-row-diamond-u')), findsNothing);
      expect(find.text('Ready for your review'), findsOneWidget);
      double y(Finder f) => tester.getTopLeft(f).dy;
      final updates = find.byKey(const ValueKey('inbox-section-updates'));
      expect(y(find.byKey(const ValueKey('inbox-section-needs-you'))) < y(find.byKey(const ValueKey('inbox-row-w'))), isTrue);
      expect(y(find.byKey(const ValueKey('inbox-row-w'))) < y(updates), isTrue);
      expect(y(find.byKey(const ValueKey('inbox-section-done'))) < y(updates), isTrue);
      expect(y(updates) <= y(find.byKey(const ValueKey('inbox-row-u'))), isTrue);
      expect(find.text('UPDATES AND DONE'), findsOneWidget);
    });

    testWidgets('with nothing at all the empty line explains what lands here', (tester) async {
      await show(tester, const InboxState(loading: false, personalOn: true));
      expect(find.textContaining("You get one when you're made responsible"), findsOneWidget);
    });

    testWidgets('an older server has no section Updates and no such text', (tester) async {
      await show(tester, InboxState(loading: false, notifications: [result('r')]), link: _link(personal: false));
      expect(find.byKey(const ValueKey('inbox-section-updates')), findsNothing);
      expect(find.text('Subject r'), findsOneWidget);
    });

    testWidgets('a failed read says so with Try again', (tester) async {
      final (_, bloc, _) = await show(tester, const InboxState(loading: false, personalOn: true, loadFailed: true));
      expect(find.text('Could not load the Inbox.'), findsOneWidget);
      await tester.tap(find.text('Try again'));
      verify(() => bloc.add(const InboxRefreshed())).called(1);
    });

    testWidgets('a tap opens the place and reads the row', (tester) async {
      final n = row('p', reason: 'plan_review');
      final (_, bloc, _) = await show(tester, InboxState(loading: false, personalOn: true, notifications: [n]));
      await tester.tap(find.byKey(const ValueKey('inbox-row-p')));
      await tester.pumpAndSettle();
      expect(find.text('plan k42'), findsOneWidget);
      verify(() => bloc.add(any(that: isA<InboxRowOpened>().having((e) => e.row.id, 'row', 'p')))).called(1);
    });

    testWidgets('+n shows only with merged_count and opens the Activity', (tester) async {
      await show(tester, InboxState(loading: false, personalOn: true, notifications: [row('a', merged: 2), row('b', ticket: 'k43')]));
      expect(find.text('+2'), findsOneWidget);
      expect(find.byKey(const ValueKey('inbox-row-more-b')), findsNothing);
      await tester.tap(find.text('+2'));
      await tester.pumpAndSettle();
      expect(find.text('ticket k42 tab activity'), findsOneWidget);
    });

    testWidgets('a channel row (chat is gone) opens the Inbox; its +n has no Activity to open', (tester) async {
      await show(tester, InboxState(loading: false, personalOn: true, notifications: [row('c', ticket: null, number: null, conversation: 'c7', subject: 'general', merged: 1)]));
      expect(find.text('# general'), findsOneWidget);
      await tester.tap(find.text('+1'));
      await tester.pumpAndSettle();
      expect(find.text('inbox'), findsOneWidget);
    });

    testWidgets('an unknown reason shows as a plain row that opens its ticket', (tester) async {
      await show(tester, InboxState(loading: false, personalOn: true, notifications: [row('x', reason: 'from_the_future', title: 'Something new happened')]));
      expect(find.text('Something new happened'), findsOneWidget);
      expect(find.byKey(const ValueKey('inbox-row-diamond-x')), findsNothing);
      await tester.tap(find.byKey(const ValueKey('inbox-row-x')));
      await tester.pumpAndSettle();
      expect(find.text('ticket k42 tab null'), findsOneWidget);
    });

    testWidgets('a swipe dismisses a row', (tester) async {
      final (_, bloc, _) = await show(tester, InboxState(loading: false, personalOn: true, notifications: [row('a')]));
      await tester.drag(find.byKey(const ValueKey('inbox-row-a')), const Offset(-800, 0));
      await tester.pumpAndSettle();
      verify(() => bloc.add(any(that: isA<NotificationDismissed>().having((e) => e.id, 'id', 'a')))).called(1);
    });

    group('a long press', () {
      Future<_MockRepository> menu(WidgetTester tester, NotificationView n, {LinkState? link, bool follow = true, bool links = true}) async {
        final (repo, _, _) = await show(tester, InboxState(loading: false, personalOn: true, followOn: follow, linksOn: links, notifications: [n]), link: link);
        await tester.longPress(find.byKey(ValueKey('inbox-row-${n.id}')));
        await tester.pumpAndSettle();
        return repo;
      }

      Receipt receipt() => const Receipt(committedSeq: 1, idempotencyKey: 'k', replayed: false, result: WriteResultCommand(value: CommandResultTicketFollowChanged(changed: true, on: false, ticketId: 'k42')));

      testWidgets('Unfollow writes and says it', (tester) async {
        final repo = await menu(tester, row('a'));
        when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry'))).thenAnswer((_) async => receipt());
        await tester.tap(find.byKey(const ValueKey('inbox-row-menu-Unfollow')));
        await tester.pumpAndSettle();
        final sent = verify(() => repo.write(captureAny(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry'))).captured.single as Map<String, dynamic>;
        expect(sent['method'], 'ticket.follow');
        expect(sent['params'], {'ticket_id': 'k42', 'on': false});
        expect(find.textContaining('Unfollowed #42.'), findsOneWidget);
      });

      testWidgets('the server refusing it for the Responsible shows its words', (tester) async {
        const refused = 'You are responsible for this ticket; hand it over before you unfollow it.';
        final repo = await menu(tester, row('a'));
        when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry'))).thenThrow(const ProtocolException(ErrorCode.validation, refused));
        await tester.tap(find.byKey(const ValueKey('inbox-row-menu-Unfollow')));
        await tester.pumpAndSettle();
        expect(find.text(refused), findsOneWidget);
      });

      testWidgets('Copy link on a ticket row gives the ticket', (tester) async {
        String? copied;
        tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
          if (call.method == 'Clipboard.setData') copied = (call.arguments as Map)['text'] as String;
          return null;
        });
        addTearDown(() => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, null));
        await menu(tester, row('a'));
        await tester.tap(find.byKey(const ValueKey('inbox-row-menu-Copy link')));
        await tester.pumpAndSettle();
        expect(copied, 'aurelius://${_host.hostId}/ticket/k42');
        expect(find.text('Link to #42 copied.'), findsOneWidget);
      });

      testWidgets('Copy link on a mention gives the message', (tester) async {
        String? copied;
        tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
          if (call.method == 'Clipboard.setData') copied = (call.arguments as Map)['text'] as String;
          return null;
        });
        addTearDown(() => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, null));
        await menu(tester, row('a', message: '01a19bd5-6957-73a6-a074-d98f1c80f0b3'));
        await tester.tap(find.byKey(const ValueKey('inbox-row-menu-Copy link')));
        await tester.pumpAndSettle();
        expect(copied, 'aurelius://${_host.hostId}/message/01a19bd5-6957-73a6-a074-d98f1c80f0b3');
      });

      testWidgets('a channel row has no Unfollow; without the features neither Copy link', (tester) async {
        await menu(tester, row('c', ticket: null, number: null, conversation: 'c7', subject: 'general'), follow: false, links: false);
        expect(find.byKey(const ValueKey('inbox-row-menu-Open')), findsOneWidget);
        expect(find.byKey(const ValueKey('inbox-row-menu-Unfollow')), findsNothing);
        expect(find.byKey(const ValueKey('inbox-row-menu-Copy link')), findsNothing);
      });
    });
  });

  group('a push (iOS)', () {
    test('of a ticket opens it at its place; of a channel the Inbox', () {
      expect(pushPlace({'target_type': 'ticket.plan_review', 'reason': 'plan_review', 'ticket_id': 'k42', 'task_id': 't1', 'project_id': 'p', 'notification_id': 'n'}), '/ticket/k42/plan');
      expect(pushPlace({'reason': 'mention', 'ticket_id': 'k42', 'message_id': 'm'}), '/ticket/k42?tab=discussion&message=m');
      expect(pushPlace({'reason': 'mention', 'conversation_id': 'c7', 'message_id': 'm'}), '/inbox');
    });

    test('an unknown reason still opens its ticket', () {
      expect(pushPlace({'reason': 'from_the_future', 'ticket_id': 'k42'}), '/ticket/k42');
    });

    test('of an older server names neither, so it keeps the old way (task, or Inbox)', () {
      expect(pushPlace({'target_type': 'task.failed', 'aggregate_id': 'a', 'task_id': 't1', 'project_id': 'p', 'notification_id': 'n'}), isNull);
    });

    test('the way never comes from a text in the push', () {
      expect(pushPlace({'title': '/ticket/evil', 'body': '/channel/evil', 'subject': 'x'}), isNull);
    });
  });
}
