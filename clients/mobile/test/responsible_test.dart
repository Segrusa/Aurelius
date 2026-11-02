import 'package:aurelius_mobile/core/net/connection.dart' show LinkState;
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/tickets.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/board_bloc.dart';
import 'package:aurelius_mobile/features/board/board_page.dart';
import 'package:aurelius_mobile/features/board/new_ticket_page.dart';
import 'package:aurelius_mobile/features/board/ticket_page.dart';
import 'package:flutter/gestures.dart' show kLongPressTimeout;
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

// A1 (US-A1, ui-ontwerp §1–§2, B8, B9): the Responsible on a card, the ticket
// page and New ticket; nothing of it on a server without `ticket_responsible`.

const _members = [testUser, bo, UserView(id: 'u-3', username: 'Alexandra', createdAtMs: 0), UserView(id: 'u-4', username: 'gone', createdAtMs: 0, disabledAtMs: 5)];

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  Widget board(MockRepository repo) => BlocProvider(create: (_) => BoardBloc(repo), child: const BoardPage());

  MockRepository repoWith(BoardView shown, {TicketDetail? detail, LinkState link = phaseALink}) {
    final repo = boardRepository(board: shown, detail: detail, link: link);
    when(() => repo.boardMembers()).thenAnswer((_) async => _members);
    return repo;
  }

  Future<void> openMenu(WidgetTester tester, String id) async {
    final gesture = await tester.startGesture(tester.getCenter(find.byKey(ValueKey('ticket-$id'))));
    await tester.pump(kLongPressTimeout + const Duration(milliseconds: 50));
    await gesture.up();
    await settleBoard(tester);
  }

  Future<void> tapKey(WidgetTester tester, String key) async {
    await tester.ensureVisible(find.byKey(ValueKey(key)));
    await tester.tap(find.byKey(ValueKey(key)));
    await settleBoard(tester);
  }

  group('the card (§1.3, B8)', () {
    Future<void> show(WidgetTester tester, TicketView ticket, {LinkState link = phaseALink}) async {
      final repo = repoWith(boardOf([ticket]), link: link);
      await pumpBoardPage(tester, repo, board(repo), link: link);
    }

    testWidgets('the who-block replaces "by …": who works on whose login, and who follows up', (tester) async {
      await show(tester, ticketOf(column: ColumnKind.doing, ownerEmployeeId: 'forge', ownerName: 'Forge', login: bo, responsible: const UserView(id: 'u-3', username: 'Alexandra', createdAtMs: 0)));
      expect(find.text('Forge'), findsOneWidget);
      expect(find.text(" · on bo's login"), findsOneWidget);
      expect(find.text('Alexandra'), findsOneWidget);
      expect(find.text(' · responsible'), findsOneWidget);
      expect(find.textContaining('by ada'), findsNothing, reason: 'B8: the creator left the card');
    });

    testWidgets('all mine reads You and your; nobody responsible is said, never a warning', (tester) async {
      await show(tester, ticketOf(column: ColumnKind.doing, ownerEmployeeId: 'forge', ownerName: 'Forge', responsible: testUser));
      expect(find.text(' · on your login'), findsOneWidget);
      expect(find.text('You'), findsOneWidget);
    });

    testWidgets('without one: No one responsible', (tester) async {
      await show(tester, ticketOf(column: ColumnKind.doing));
      expect(find.text('No one responsible'), findsOneWidget);
      expect(find.text('No owner'), findsOneWidget);
    });

    testWidgets('a disabled responsible stays visible, in words', (tester) async {
      await show(tester, ticketOf(column: ColumnKind.doing, responsible: bo, responsibleDisabled: true));
      expect(find.text('bo (disabled)'), findsOneWidget);
    });

    testWidgets('an older server keeps today\'s row with by … and shows no responsible', (tester) async {
      await show(tester, ticketOf(column: ColumnKind.doing, ownerEmployeeId: 'forge', ownerName: 'Forge'), link: onlineLink);
      expect(find.text('by ada'), findsOneWidget);
      expect(find.textContaining('responsible'), findsNothing);
      expect(find.byKey(const ValueKey('board-filter-responsible')), findsNothing);
    });

    testWidgets('a screen reader hears who does what as one sentence', (tester) async {
      final handle = tester.ensureSemantics();
      await show(tester, ticketOf(column: ColumnKind.doing, ownerEmployeeId: 'forge', ownerName: 'Forge', login: bo, responsible: testUser));
      expect(find.bySemanticsLabel(RegExp("Forge does the work. Runs on bo's Claude login. You are responsible: follows up and reviews. Created by you.")), findsOneWidget);
      handle.dispose();
    });
  });

  group('the card\'s menu (§2.2)', () {
    testWidgets('Make me responsible writes me, with the ticket\'s version; the toast has no Undo', (tester) async {
      final repo = repoWith(boardOf([ticketOf(number: 42, column: ColumnKind.doing)]));
      await pumpBoardPage(tester, repo, board(repo), link: phaseALink);
      await openMenu(tester, 'k1');
      expect(find.byKey(const ValueKey('card-menu-creator')), findsOneWidget, reason: 'B8: the creator is the menu\'s head line');
      await tester.tap(find.byKey(const ValueKey('card-menu-item-Make me responsible')));
      await settleBoard(tester);
      final write = writesOf(repo).single;
      expect(write['method'], 'ticket.set_responsible');
      expect(paramsOf(write), {'ticket_id': 'k1', 'responsible_user_id': 'u-1'});
      expect(find.text('You are now responsible for #42.'), findsOneWidget);
      expect(find.byType(SnackBarAction), findsNothing);
    });

    testWidgets('Remove me as responsible leaves no one', (tester) async {
      final repo = repoWith(boardOf([ticketOf(number: 42, column: ColumnKind.doing, responsible: testUser)]));
      await pumpBoardPage(tester, repo, board(repo), link: phaseALink);
      await openMenu(tester, 'k1');
      await tester.tap(find.byKey(const ValueKey('card-menu-item-Remove me as responsible')));
      await settleBoard(tester);
      expect(paramsOf(writesOf(repo).single), {'ticket_id': 'k1'});
      expect(find.text('No one is responsible for #42 now.'), findsOneWidget);
    });

    testWidgets('Set responsible…: You, the active ones A–Z, never the disabled; handing over names them', (tester) async {
      final repo = repoWith(boardOf([ticketOf(number: 42, column: ColumnKind.doing, responsible: testUser)]));
      await pumpBoardPage(tester, repo, board(repo), link: phaseALink);
      await openMenu(tester, 'k1');
      await tester.tap(find.byKey(const ValueKey('card-menu-item-Set responsible…')));
      await settleBoard(tester);
      expect(find.text(responsibleExplained), findsOneWidget);
      expect(find.byKey(const ValueKey('responsible-u-4')), findsNothing, reason: 'P-78');
      expect(find.byKey(const ValueKey('responsible-no-one')), findsOneWidget);
      final you = tester.getTopLeft(find.byKey(const ValueKey('responsible-u-1'))).dy;
      final alexandra = tester.getTopLeft(find.byKey(const ValueKey('responsible-u-3'))).dy;
      final others = tester.getTopLeft(find.byKey(const ValueKey('responsible-u-2'))).dy;
      expect(you < alexandra && alexandra < others, isTrue);
      await tester.tap(find.byKey(const ValueKey('responsible-u-3')));
      await settleBoard(tester);
      expect(paramsOf(writesOf(repo).single), {'ticket_id': 'k1', 'responsible_user_id': 'u-3'});
      expect(find.text('Alexandra is now responsible for #42.'), findsOneWidget);
    });

    testWidgets('No one is only offered when someone is responsible', (tester) async {
      final repo = repoWith(boardOf([ticketOf(column: ColumnKind.doing)]));
      await pumpBoardPage(tester, repo, board(repo), link: phaseALink);
      await openMenu(tester, 'k1');
      await tester.tap(find.byKey(const ValueKey('card-menu-item-Set responsible…')));
      await settleBoard(tester);
      expect(find.byKey(const ValueKey('responsible-no-one')), findsNothing);
    });

    testWidgets('a Done ticket offers none of it', (tester) async {
      final repo = repoWith(boardOf([ticketOf(id: 'k2', column: ColumnKind.done)]));
      await pumpBoardPage(tester, repo, board(repo), link: phaseALink);
      await tapKey(tester, 'board-chip-done');
      await tester.pumpAndSettle();
      await openMenu(tester, 'k2');
      expect(find.byKey(const ValueKey('card-menu-item-Set responsible…')), findsNothing);
    });

    testWidgets('an older server\'s menu has neither Responsible nor the creator line', (tester) async {
      final repo = repoWith(boardOf([ticketOf(column: ColumnKind.doing)]), link: onlineLink);
      await pumpBoardPage(tester, repo, board(repo));
      await openMenu(tester, 'k1');
      expect(find.byKey(const ValueKey('card-menu-item-Set responsible…')), findsNothing);
      expect(find.byKey(const ValueKey('card-menu-item-Make me responsible')), findsNothing);
      expect(find.byKey(const ValueKey('card-menu-creator')), findsNothing);
    });
  });

  group('the filter (P-77)', () {
    testWidgets('Responsible: Me, No one, and a name; Owner: gets its prefix', (tester) async {
      final mine = ticketOf(id: 'k-mine', number: 1, column: ColumnKind.doing, responsible: testUser);
      final theirs = ticketOf(id: 'k-theirs', number: 2, column: ColumnKind.doing, responsible: bo);
      final nobodys = ticketOf(id: 'k-none', number: 3, column: ColumnKind.doing);
      final repo = repoWith(boardOf([mine, theirs, nobodys]));
      await pumpBoardPage(tester, repo, board(repo), link: phaseALink);
      expect(find.text('Responsible: Anyone'), findsOneWidget);
      expect(find.text('Owner: Anyone'), findsOneWidget);

      Future<void> pick(String key) async {
        await tapKey(tester, 'board-filter-responsible');
        await tester.tap(find.byKey(ValueKey(key)));
        await settleBoard(tester);
      }

      await pick('board-responsible-me');
      expect(find.text('Responsible: Me'), findsOneWidget);
      expect(find.byKey(const ValueKey('ticket-k-mine')), findsOneWidget);
      expect(find.byKey(const ValueKey('ticket-k-theirs')), findsNothing);

      await pick('board-responsible-no-one');
      expect(find.byKey(const ValueKey('ticket-k-none')), findsOneWidget);
      expect(find.byKey(const ValueKey('ticket-k-mine')), findsNothing);

      await pick('board-responsible-u-2');
      expect(find.text('Responsible: bo'), findsOneWidget);
      expect(find.byKey(const ValueKey('ticket-k-theirs')), findsOneWidget);
    });

    testWidgets('a disabled board member with tickets is offered under Disabled; one without is not', (tester) async {
      final repo = repoWith(boardOf([ticketOf(id: 'k-off', column: ColumnKind.doing, responsible: const UserView(id: 'u-4', username: 'gone', createdAtMs: 0), responsibleDisabled: true)]));
      when(() => repo.boardMembers()).thenAnswer((_) async => [..._members, const UserView(id: 'u-5', username: 'away', createdAtMs: 0, disabledAtMs: 5)]);
      await pumpBoardPage(tester, repo, board(repo), link: phaseALink);
      await tapKey(tester, 'board-filter-responsible');
      expect(find.text('DISABLED'), findsOneWidget);
      expect(find.byKey(const ValueKey('board-responsible-u-4')), findsOneWidget);
      expect(find.byKey(const ValueKey('board-responsible-u-5')), findsNothing);
    });
  });

  group('the ticket page (§1.4, §2.5)', () {
    TicketDetail detailOf(TicketView ticket) => TicketDetail(asOfSeq: 1, ticket: ticket, tasks: const []);

    Future<MockRepository> open(WidgetTester tester, TicketView ticket, {LinkState link = phaseALink}) async {
      final repo = repoWith(boardOf([ticket]), detail: detailOf(ticket), link: link);
      when(() => repo.followers(any())).thenAnswer((_) async => const TicketFollowersView(followers: []));
      await pumpBoardPage(tester, repo, TicketPage(ticketId: ticket.id), link: link);
      return repo;
    }

    testWidgets('the who-strip and the block Who: Responsible with its explanation, then Owner, Runs on, Created by', (tester) async {
      await open(tester, ticketOf(number: 42, responsible: bo));
      expect(find.byKey(const ValueKey('who-responsible-strip')), findsOneWidget);
      expect(find.byKey(const ValueKey('ticket-row-responsible')), findsOneWidget);
      expect(find.text(responsibleExplained), findsOneWidget);
      final rows = ['ticket-row-responsible', 'ticket-row-owner', 'ticket-row-login'].map((k) => tester.getTopLeft(find.byKey(ValueKey(k))).dy).toList();
      expect(rows[0] < rows[1] && rows[1] < rows[2], isTrue);
    });

    testWidgets('choosing in the row writes it', (tester) async {
      final repo = await open(tester, ticketOf(number: 42));
      await tapKey(tester, 'ticket-row-responsible');
      await tester.tap(find.byKey(const ValueKey('responsible-u-2')));
      await settleBoard(tester);
      expect(writesOf(repo).single['method'], 'ticket.set_responsible');
      expect(find.text('bo is now responsible for #42.'), findsOneWidget);
    });

    testWidgets('nobody responsible offers Make me responsible', (tester) async {
      final repo = await open(tester, ticketOf(number: 42));
      await tapKey(tester, 'ticket-make-me-responsible');
      expect(paramsOf(writesOf(repo).single), {'ticket_id': 'k1', 'responsible_user_id': 'u-1'});
    });

    testWidgets('a version conflict says who is responsible now and overwrites nothing', (tester) async {
      final repo = await open(tester, ticketOf(number: 42));
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenThrow(const ProtocolException(ErrorCode.versionConflict, 'stale'));
      when(() => repo.ticket(any())).thenAnswer((_) async => detailOf(ticketOf(number: 42, responsible: bo)));
      await tapKey(tester, 'ticket-make-me-responsible');
      expect(find.text('Someone changed this first. bo is responsible now.'), findsOneWidget);
    });

    testWidgets('on Done it is read-only and says why', (tester) async {
      await open(tester, ticketOf(number: 42, column: ColumnKind.done, responsible: bo));
      await tester.ensureVisible(find.byKey(const ValueKey('ticket-row-responsible')));
      await tester.tap(find.byKey(const ValueKey('ticket-row-responsible')));
      await settleBoard(tester);
      expect(find.byKey(const ValueKey('responsible-u-2')), findsNothing);
      expect(find.text('Done tickets are closed.'), findsOneWidget);
    });

    testWidgets('a disabled responsible asks to choose someone else', (tester) async {
      await open(tester, ticketOf(number: 42, responsible: bo, responsibleDisabled: true));
      expect(find.text('bo is disabled and gets no updates. Choose someone else.'), findsOneWidget);
    });

    testWidgets('someone else\'s review: no ◆, the bar says so and keeps Done and Send back', (tester) async {
      await open(tester, ticketOf(number: 42, column: ColumnKind.review, responsible: bo));
      expect(find.text('bo is responsible. You can decide too.'), findsOneWidget);
      expect(find.byKey(const ValueKey('bar-done')), findsOneWidget);
      expect(find.byKey(const ValueKey('bar-send-back')), findsOneWidget);
      expect(find.text('◆ Report to review'), findsNothing);
      expect(find.text('Report to review'), findsOneWidget);
    });

    testWidgets('my own review keeps the ◆ and no note', (tester) async {
      await open(tester, ticketOf(number: 42, column: ColumnKind.review, responsible: testUser));
      expect(find.text('◆ Report to review'), findsOneWidget);
      expect(find.byKey(const ValueKey('bar-others-note')), findsNothing);
    });

    testWidgets('an older server shows no who-strip or block, and its rows stay in Details', (tester) async {
      await open(tester, ticketOf(number: 42), link: onlineLink);
      expect(find.byKey(const ValueKey('who-responsible-strip')), findsNothing);
      expect(find.byKey(const ValueKey('ticket-row-responsible')), findsNothing);
      expect(find.text('WHO'), findsNothing);
      expect(find.byKey(const ValueKey('ticket-row-owner')), findsOneWidget);
    });
  });

  group('New ticket (B9)', () {
    Future<MockRepository> open(WidgetTester tester, {LinkState link = phaseALink}) async {
      final repo = repoWith(boardOf(const []), link: link);
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((_) async => receiptOf(const CommandResultTicketCreated(number: 52, ticketId: 'k52')));
      await pumpBoardPage(tester, repo, const NewTicketPage(projectId: 'p1'), link: link);
      await tester.enterText(find.byKey(const ValueKey('new-ticket-title')), 'A title');
      await tester.pump();
      return repo;
    }

    testWidgets('it starts on You and creates with me responsible', (tester) async {
      final repo = await open(tester);
      expect(find.descendant(of: find.byKey(const ValueKey('new-ticket-responsible')), matching: find.text('You')), findsOneWidget);
      await tapKey(tester, 'new-ticket-create');
      expect(paramsOf(writesOf(repo).single)['responsible_user_id'], 'u-1');
    });

    testWidgets('No one stays a choice and sends none', (tester) async {
      final repo = await open(tester);
      await tapKey(tester, 'new-ticket-responsible');
      await tester.tap(find.byKey(const ValueKey('responsible-no-one')));
      await settleBoard(tester);
      expect(find.descendant(of: find.byKey(const ValueKey('new-ticket-responsible')), matching: find.text('No one')), findsOneWidget);
      await tapKey(tester, 'new-ticket-create');
      expect(paramsOf(writesOf(repo).single).containsKey('responsible_user_id'), isFalse);
    });

    testWidgets('another board member can be chosen', (tester) async {
      final repo = await open(tester);
      await tapKey(tester, 'new-ticket-responsible');
      await tester.tap(find.byKey(const ValueKey('responsible-u-2')));
      await settleBoard(tester);
      await tapKey(tester, 'new-ticket-create');
      expect(paramsOf(writesOf(repo).single)['responsible_user_id'], 'u-2');
    });

    testWidgets('an older server gets no row and no field, which it would refuse', (tester) async {
      final repo = await open(tester, link: onlineLink);
      expect(find.byKey(const ValueKey('new-ticket-responsible')), findsNothing);
      await tapKey(tester, 'new-ticket-create');
      expect(paramsOf(writesOf(repo).single).containsKey('responsible_user_id'), isFalse);
    });
  });

  test('responsibleChoices: me first, the active ones A–Z, nobody disabled', () {
    expect(responsibleChoices(_members, me: 'u-1').map((c) => c.label).toList(), ['You', 'Alexandra', 'bo']);
  });

}
