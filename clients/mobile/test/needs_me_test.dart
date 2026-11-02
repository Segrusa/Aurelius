import 'package:aurelius_mobile/core/widgets/widgets.dart';
import 'package:aurelius_mobile/data/tickets.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/board_bloc.dart';
import 'package:aurelius_mobile/features/board/board_page.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/tickets.dart';

// B6 (freeze §1): the rule of the ◆, the filter, the count and Waiting on me.
// The same cases as the desktop's `ticket_needs_me`.

const _me = 'u-1';

TicketView _ticket({UserView? responsible, bool? disabled, TicketNeed? needs, UserView? login, ColumnKind column = ColumnKind.review}) =>
    ticketOf(column: column, needs: needs, login: login, responsible: responsible, responsibleDisabled: disabled);

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  const decisions = [TicketState.needsOwner, TicketState.ownerUnknown, TicketState.waitingForYou, TicketState.failed, TicketState.planReview, TicketState.review];

  group('needsMe', () {
    for (final state in decisions) {
      test('$state without a responsible is everyone\'s, as on an older server', () {
        expect(needsMe(_ticket(), state, me: _me), isTrue);
      });

      test('$state with me responsible is mine', () {
        expect(needsMe(_ticket(responsible: testUser), state, me: _me), isTrue);
      });

      test('$state with another responsible is not mine', () {
        expect(needsMe(_ticket(responsible: bo), state, me: _me), isFalse);
      });

      test('$state with a disabled responsible is everyone\'s again', () {
        expect(needsMe(_ticket(responsible: bo, disabled: true), state, me: _me), isTrue);
      });
    }

    test('a wait for my own login is mine, whoever is responsible', () {
      final ticket = _ticket(responsible: bo, needs: TicketNeed.login);
      expect(needsMe(ticket, TicketState.needsYourLogin, me: _me), isTrue);
    });

    test('a wait for someone else\'s login is not mine, also when I am responsible', () {
      final ticket = _ticket(responsible: testUser, needs: TicketNeed.login, login: bo);
      expect(needsMe(ticket, TicketState.waitsForLogin, me: _me), isFalse);
    });

    test('a wait for someone else\'s login is not mine without a responsible either', () {
      final ticket = _ticket(needs: TicketNeed.login, login: bo, column: ColumnKind.doing);
      expect(needsMe(ticket, ticketState(ticket, me: _me), me: _me), isFalse);
    });

    for (final state in [TicketState.backlog, TicketState.choosing, TicketState.queued, TicketState.working, TicketState.waitingForLimit, TicketState.testing, TicketState.done]) {
      test('$state needs nobody, also when I am responsible', () {
        expect(needsMe(_ticket(responsible: testUser), state, me: _me), isFalse);
      });
    }

    test('the state keeps showing a decision to someone who is not responsible: anyone can still decide', () {
      final ticket = _ticket(responsible: bo);
      final state = ticketState(ticket, me: _me);
      expect(state.needsYou, isTrue, reason: 'the card keeps its button and line');
      expect(needsMe(ticket, state, me: _me), isFalse);
    });
  });

  group('waitsForOther', () {
    test('names the responsible when the decision is theirs', () {
      expect(waitsForOther(_ticket(responsible: bo), TicketState.waitingForYou, me: _me), 'bo');
      expect(TicketState.waitingForYou.line(waitsFor: 'bo'), 'Waiting for bo');
    });

    test('is null when the decision is mine', () {
      expect(waitsForOther(_ticket(responsible: testUser), TicketState.waitingForYou, me: _me), isNull);
      expect(TicketState.waitingForYou.line(), 'Waiting for you');
    });
  });

  group('on the Board', () {
    Widget page(MockRepository repo) => BlocProvider(create: (_) => BoardBloc(repo), child: const BoardPage());
    final mine = ticketOf(id: 'k-mine', number: 1, column: ColumnKind.review, responsible: testUser);
    final theirs = ticketOf(id: 'k-theirs', number: 2, title: 'Their ticket', column: ColumnKind.review, responsible: bo);
    final nobodys = ticketOf(id: 'k-nobodys', number: 3, title: 'Nobody\'s ticket', column: ColumnKind.review);
    final disabled = ticketOf(id: 'k-off', number: 4, title: 'Off ticket', column: ColumnKind.review, responsible: bo, responsibleDisabled: true);

    testWidgets('the count and the ◆ follow needsMe; the other\'s card keeps its button', (tester) async {
      final repo = boardRepository(board: boardOf([mine, theirs, nobodys, disabled]), link: phaseALink);
      await pumpBoardPage(tester, repo, page(repo), link: phaseALink);
      expect(find.text('4 tickets · 3 need you'), findsOneWidget);
      expect(find.byKey(const ValueKey('card-action-k-theirs')), findsOneWidget, reason: 'anyone can still decide');
      expect(find.descendant(of: find.byKey(const ValueKey('ticket-k-theirs')), matching: find.text('◆')), findsNothing);
      expect(find.descendant(of: find.byKey(const ValueKey('ticket-k-mine')), matching: find.text('◆')), findsOneWidget);
    });

    testWidgets('the filter Needs you leaves only what counts as mine', (tester) async {
      final repo = boardRepository(board: boardOf([theirs, ticketOf(id: 'k-mine', number: 1, column: ColumnKind.review, responsible: testUser)]), link: phaseALink);
      await pumpBoardPage(tester, repo, page(repo), link: phaseALink);
      await tester.tap(find.byKey(const ValueKey('board-filter-needs-you')));
      await settleBoard(tester);
      expect(find.byKey(const ValueKey('ticket-k-theirs')), findsNothing);
      expect(find.byKey(const ValueKey('ticket-k-mine')), findsOneWidget);
    });

    testWidgets('a Review full of someone else\'s tickets shows no dot on its chip', (tester) async {
      final repo = boardRepository(board: boardOf([theirs]), link: phaseALink);
      await pumpBoardPage(tester, repo, page(repo), link: phaseALink);
      expect(find.descendant(of: find.byKey(const ValueKey('board-chip-review')), matching: find.byType(StatusDot)), findsNothing);
    });

    testWidgets('an older server shows no filter, and every decision counts for me as before', (tester) async {
      final repo = boardRepository(board: boardOf([nobodys]));
      await pumpBoardPage(tester, repo, page(repo));
      expect(find.byKey(const ValueKey('board-filter-needs-you')), findsNothing);
      expect(find.text('1 ticket · 1 needs you'), findsOneWidget);
      expect(find.descendant(of: find.byKey(const ValueKey('board-chip-review')), matching: find.byType(StatusDot)), findsOneWidget);
    });
  });
}
