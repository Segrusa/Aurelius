import 'dart:async';

import 'package:aurelius_mobile/core/net/connection.dart' show LinkState, LinkStatus;
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/tickets.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/board_bloc.dart';
import 'package:aurelius_mobile/features/board/board_page.dart';
import 'package:aurelius_mobile/features/board/ticket_bloc.dart';
import 'package:aurelius_mobile/features/board/ticket_page.dart';
import 'package:flutter/gestures.dart' show kLongPressTimeout;
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

// A2 (US-A2, ui-ontwerp §3, B10): following a ticket; hidden on a server
// without `ticket_follow`.

FollowerView _follower(UserView user, String reason, {bool disabled = false}) => FollowerView(disabled: disabled, followedAtMs: 0, reason: reason, userId: user.id, username: user.username);

/// Only `ticket_follow`: the block Who shows without the Responsible's fields.
const _followOnly = LinkState(status: LinkStatus.online, capabilities: ['read', 'chat', 'task_control', 'approve'], features: ['ticket_follow'], boardMembersExist: true, user: testUser, claudeLogin: testLogin);

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  TicketDetail detailOf(TicketView ticket) => TicketDetail(asOfSeq: 1, ticket: ticket, tasks: const []);

  Future<MockRepository> open(WidgetTester tester, TicketView ticket, List<FollowerView> followers, {LinkState link = phaseALink}) async {
    final repo = boardRepository(detail: detailOf(ticket), link: link);
    when(() => repo.followers(any())).thenAnswer((_) async => TicketFollowersView(followers: followers));
    await pumpBoardPage(tester, repo, TicketPage(ticketId: ticket.id), link: link);
    // The ticket's data is in its properties (ticket #71).
    await openTicketProperties(tester);
    return repo;
  }

  Future<void> toggle(WidgetTester tester) async {
    await tester.ensureVisible(find.byKey(const ValueKey('ticket-following')));
    await tester.pump();
    await tester.tap(find.byType(Switch));
    await settleBoard(tester);
  }

  Switch shownSwitch(WidgetTester tester) => tester.widget<Switch>(find.byType(Switch));

  group('on the ticket page (§3.2)', () {
    testWidgets('Follow writes ticket.follow on, says what it means and reads the list again', (tester) async {
      final repo = await open(tester, ticketOf(number: 42), const []);
      expect(shownSwitch(tester).value, isFalse);
      await toggle(tester);
      final write = writesOf(repo).single;
      expect(write['method'], 'ticket.follow');
      expect(paramsOf(write), {'ticket_id': 'k1', 'on': true});
      expect(find.text("You'll get updates on #42."), findsOneWidget);
      verify(() => repo.followers('k1')).called(greaterThan(1));
    });

    testWidgets('Unfollow writes ticket.follow off and says how following comes back', (tester) async {
      final repo = await open(tester, ticketOf(number: 42), [_follower(testUser, 'manual')]);
      expect(shownSwitch(tester).value, isTrue);
      await toggle(tester);
      expect(paramsOf(writesOf(repo).single), {'ticket_id': 'k1', 'on': false});
      expect(find.text('Unfollowed #42. A new @mention or becoming responsible makes you follow again.'), findsOneWidget);
    });

    for (final (reason, text) in [
      ('created', 'You follow because you created it.'),
      ('mentioned', 'You follow because you were @mentioned.'),
      ('manual', 'You follow because you chose to.'),
      ('a_future_reason', 'You follow this ticket.'),
    ]) {
      testWidgets('following because of "$reason" reads: $text', (tester) async {
        await open(tester, ticketOf(number: 42), [_follower(testUser, reason)]);
        expect(find.text(text), findsOneWidget);
      });
    }

    testWidgets('the responsible cannot unfollow (B10): the switch stays on and off limits, and says why', (tester) async {
      final repo = await open(tester, ticketOf(number: 42, responsible: testUser), [_follower(testUser, 'responsible')]);
      expect(shownSwitch(tester).value, isTrue);
      expect(shownSwitch(tester).onChanged, isNull);
      expect(find.text("You're responsible for #42. Hand it to someone else to stop following."), findsOneWidget);
      await tester.ensureVisible(find.byKey(const ValueKey('ticket-following')));
      await tester.pump();
      await tester.tap(find.byType(Switch));
      await settleBoard(tester);
      verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
    });

    testWidgets('after handing over the old responsible can unfollow', (tester) async {
      await open(tester, ticketOf(number: 42, responsible: bo), [_follower(testUser, 'responsible')]);
      expect(shownSwitch(tester).onChanged, isNotNull);
    });

    testWidgets('on Done it keeps working and says no updates come', (tester) async {
      final repo = await open(tester, ticketOf(number: 42, column: ColumnKind.done), const []);
      expect(find.text('Done tickets send no updates.'), findsOneWidget);
      await toggle(tester);
      expect(paramsOf(writesOf(repo).single), {'ticket_id': 'k1', 'on': true});
    });

    testWidgets('a refused write leaves the switch as it was and says why', (tester) async {
      final repo = await open(tester, ticketOf(number: 42), const []);
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenThrow(const ProtocolException(ErrorCode.validation, 'This server does not do this yet.'));
      await toggle(tester);
      expect(find.text('This server does not do this yet.'), findsOneWidget);
      expect(shownSwitch(tester).value, isFalse);
    });

    testWidgets('offline the switch is off', (tester) async {
      await open(tester, ticketOf(number: 42), const [], link: const LinkState(status: LinkStatus.offline, features: ['ticket_follow'], boardMembersExist: true, user: testUser, claudeLogin: testLogin));
      expect(shownSwitch(tester).onChanged, isNull);
    });

    testWidgets('a failed read says so, offers Try again and leaves the switch alone', (tester) async {
      final repo = boardRepository(detail: detailOf(ticketOf(number: 42)), link: phaseALink);
      when(() => repo.followers(any())).thenThrow(const ProtocolException(ErrorCode.validation, 'This server does not do this yet.'));
      await pumpBoardPage(tester, repo, const TicketPage(ticketId: 'k1'), link: phaseALink);
      await openTicketProperties(tester);
      expect(find.text('Could not load · Try again'), findsOneWidget);
      expect(shownSwitch(tester).onChanged, isNull);
      clearInteractions(repo);
      when(() => repo.followers(any())).thenAnswer((_) async => TicketFollowersView(followers: [_follower(bo, 'created')]));
      await tester.ensureVisible(find.byKey(const ValueKey('ticket-row-followers')));
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('ticket-row-followers')));
      await settleBoard(tester);
      expect(find.text('Could not load · Try again'), findsNothing);
      expect(shownSwitch(tester).onChanged, isNotNull);
    });

    testWidgets('the list of followers: names, reasons, disabled in words, and empty', (tester) async {
      await open(tester, ticketOf(number: 42), [_follower(testUser, 'created'), _follower(bo, 'mentioned', disabled: true), _follower(const UserView(id: 'u-3', username: 'cy', createdAtMs: 0), 'a_future_reason')]);
      await tester.ensureVisible(find.byKey(const ValueKey('ticket-row-followers')));
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('ticket-row-followers')));
      await settleBoard(tester);
      expect(find.text('ada'), findsOneWidget);
      expect(find.text('Created it'), findsOneWidget);
      expect(find.text('bo (disabled)'), findsOneWidget);
      expect(find.text('@mentioned'), findsOneWidget);
      expect(find.text('cy'), findsOneWidget);
    });

    testWidgets('nobody follows: the sheet says it', (tester) async {
      await open(tester, ticketOf(number: 42), const []);
      await tester.ensureVisible(find.byKey(const ValueKey('ticket-row-followers')));
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('ticket-row-followers')));
      await settleBoard(tester);
      expect(find.text('No one follows this ticket.'), findsOneWidget);
    });

    testWidgets('with only ticket_follow the block has just Followers and Following', (tester) async {
      await open(tester, ticketOf(number: 42), const [], link: _followOnly);
      expect(find.byKey(const ValueKey('ticket-following')), findsOneWidget);
      expect(find.byKey(const ValueKey('ticket-row-responsible')), findsNothing);
    });

    testWidgets('an older server: no switch, no row, and no read of a method it lacks', (tester) async {
      final repo = boardRepository(detail: detailOf(ticketOf(number: 42)));
      await pumpBoardPage(tester, repo, const TicketPage(ticketId: 'k1'));
      await openTicketProperties(tester);
      expect(find.byKey(const ValueKey('ticket-following')), findsNothing);
      expect(find.byKey(const ValueKey('ticket-row-followers')), findsNothing);
      verifyNever(() => repo.followers(any()));
    });
  });

  group('the card\'s menu (§3.2)', () {
    Future<MockRepository> menu(WidgetTester tester, TicketView ticket, Future<TicketFollowersView> Function() followers, {LinkState link = phaseALink}) async {
      final repo = boardRepository(board: boardOf([ticket]), link: link);
      when(() => repo.followers(any())).thenAnswer((_) => followers());
      await pumpBoardPage(tester, repo, BlocProvider(create: (_) => BoardBloc(repo), child: const Scaffold(body: BoardPage())), link: link);
      final gesture = await tester.startGesture(tester.getCenter(find.byKey(ValueKey('ticket-${ticket.id}'))));
      await tester.pump(kLongPressTimeout + const Duration(milliseconds: 50));
      await gesture.up();
      await settleBoard(tester);
      return repo;
    }

    testWidgets('Follow when I do not; it writes', (tester) async {
      final repo = await menu(tester, ticketOf(column: ColumnKind.doing), () async => const TicketFollowersView(followers: []));
      await tester.tap(find.byKey(const ValueKey('card-menu-item-Follow')));
      await settleBoard(tester);
      expect(paramsOf(writesOf(repo).single), {'ticket_id': 'k1', 'on': true});
    });

    testWidgets('Unfollow when I do', (tester) async {
      await menu(tester, ticketOf(column: ColumnKind.doing), () async => TicketFollowersView(followers: [_follower(testUser, 'manual')]));
      expect(find.byKey(const ValueKey('card-menu-item-Unfollow')), findsOneWidget);
    });

    testWidgets('the responsible gets no Unfollow (B10)', (tester) async {
      await menu(tester, ticketOf(column: ColumnKind.doing, responsible: testUser), () async => TicketFollowersView(followers: [_follower(testUser, 'responsible')]));
      expect(find.byKey(const ValueKey('card-menu-item-Unfollow')), findsNothing);
      expect(find.byKey(const ValueKey('card-menu-item-Follow')), findsNothing);
    });

    testWidgets('a failed read leaves the item out; the rest of the menu opens', (tester) async {
      await menu(tester, ticketOf(column: ColumnKind.doing), () async => throw const ProtocolException(ErrorCode.validation, 'no'));
      expect(find.byKey(const ValueKey('card-menu-item-Open')), findsOneWidget);
      expect(find.byKey(const ValueKey('card-menu-item-Follow')), findsNothing);
    });

    testWidgets('an older server: no item and no read', (tester) async {
      final repo = await menu(tester, ticketOf(column: ColumnKind.doing), () async => const TicketFollowersView(followers: []), link: onlineLink);
      expect(find.byKey(const ValueKey('card-menu-item-Follow')), findsNothing);
      verifyNever(() => repo.followers(any()));
    });
  });

  group('ticket.follow_changed', () {
    test('the ticket reads its followers again', () async {
      final changes = StreamController<Event>.broadcast();
      addTearDown(changes.close);
      final repo = boardRepository(detail: detailOf(ticketOf()), link: phaseALink);
      when(() => repo.boardChanges).thenAnswer((_) => changes.stream);
      when(() => repo.followers(any())).thenAnswer((_) async => const TicketFollowersView(followers: []));
      final bloc = TicketBloc(repo, 'k1');
      addTearDown(bloc.close);
      await stateWhere(bloc, (s) => s.followers != null);
      when(() => repo.followers(any())).thenAnswer((_) async => TicketFollowersView(followers: [_follower(bo, 'manual')]));
      changes.add(const Event(aggregateId: 'k1', atMs: 0, kind: EventKind.ticketFollowChanged, seq: 12));
      final next = await stateWhere(bloc, (s) => s.followers?.length == 1);
      expect(next.followers!.single.username, 'bo');
    });

    test('the Board does not read itself again: nothing on a card changed', () async {
      final changes = StreamController<Event>.broadcast();
      addTearDown(changes.close);
      final repo = boardRepository(board: boardOf([ticketOf()]), link: phaseALink);
      when(() => repo.boardChanges).thenAnswer((_) => changes.stream);
      final bloc = BoardBloc(repo);
      addTearDown(bloc.close);
      await stateWhere(bloc, (s) => !s.loading);
      clearInteractions(repo);
      changes.add(const Event(aggregateId: 'k1', atMs: 0, kind: EventKind.ticketFollowChanged, seq: 12));
      await Future<void>.delayed(const Duration(milliseconds: 400));
      verifyNever(() => repo.board(projectId: any(named: 'projectId')));
    });
  });

  test('canUnfollow: everyone but the responsible', () {
    expect(canUnfollow(ticketOf(), me: 'u-1'), isTrue);
    expect(canUnfollow(ticketOf(responsible: bo), me: 'u-1'), isTrue);
    expect(canUnfollow(ticketOf(responsible: testUser), me: 'u-1'), isFalse);
  });
}
