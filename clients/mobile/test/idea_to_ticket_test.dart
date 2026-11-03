import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/discussion.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/new_ticket_page.dart';
import 'package:aurelius_mobile/features/dashboard/dashboard_page.dart';
import 'package:aurelius_mobile/features/dashboard/give_work.dart';
import 'package:aurelius_mobile/features/ideas/idea_sheet.dart';
import 'package:aurelius_mobile/features/ideas/ideas_bloc.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  MockRepository created() {
    final repo = boardRepository();
    when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
        .thenAnswer((_) async => receiptOf(const CommandResultTicketCreated(number: 52, ticketId: 'k52')));
    return repo;
  }

  group('a ticket from an idea (ticket #16)', () {
    test('the draft has its title, note and project, and a bug keeps that word', () {
      final draft = draftFromIdea(ideaOf('i1', 'Dark mode', projectId: 'p2', note: 'At night', kind: IdeaKind.bug));
      expect((draft.title, draft.description, draft.projectId, draft.fromIdea, draft.labels), ('Dark mode', 'At night', 'p2', 'i1', 'bug'));
      expect(draftFromIdea(ideaOf('i1', 'x', projectId: 'p2')).labels, '', reason: 'a feature gets no label');
    });

    test('an idea becomes a ticket once, and not after it became a task', () {
      expect(ideaCanBecomeTicket(ideaOf('i', 'x', projectId: 'p')), isTrue);
      expect(ideaCanBecomeTicket(ideaOf('i', 'x', projectId: 'p', ticketId: 'k')), isFalse);
      expect(ideaCanBecomeTicket(ideaOf('i', 'x', projectId: 'p', taskId: 't')), isFalse);
    });

    testWidgets('the form sends from_idea, stays in the idea\'s project and says what happens', (tester) async {
      final repo = created();
      final two = companyWith(projects: [
        ProjectSummary(createdAtMs: 0, id: 'p1', name: 'Atlas', status: ProjectStatus.active, version: 1, employeesActive: 0),
        ProjectSummary(createdAtMs: 0, id: 'p2', name: 'Borealis', status: ProjectStatus.active, version: 1, employeesActive: 0),
      ]);
      await pumpBoardPage(tester, repo, NewTicketPage(draft: draftFromIdea(ideaOf('i1', 'Dark mode', projectId: 'p2', note: 'At night', kind: IdeaKind.bug))), company: two);
      expect(find.byKey(const ValueKey('new-ticket-from-idea')), findsOneWidget);
      expect(tester.widget<ChoiceChip>(find.byKey(const ValueKey('new-ticket-project-p1'))).onSelected, isNull, reason: 'the server refuses another project');
      await tester.tap(find.byKey(const ValueKey('new-ticket-create')));
      await settleBoard(tester);
      final params = paramsOf(writesOf(repo).single);
      expect((params['from_idea'], params['project_id'], params['title'], params['description']), ('i1', 'p2', 'Dark mode', 'At night'));
      expect(params['labels'], ['bug']);
    });
  });

  group('the idea\'s sheet', () {
    Future<void> sheet(WidgetTester tester, IdeaView idea, {bool canMakeTicket = true}) async {
      final repo = boardRepository();
      when(() => repo.companyChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.ideas('p')).thenAnswer((_) async => [idea]);
      final bloc = IdeasBloc(repo, 'p');
      await tester.pumpWidget(MaterialApp(theme: aureliusTheme(), home: Scaffold(body: BlocProvider.value(value: bloc, child: IdeaSheet(idea: idea, canMakeTicket: canMakeTicket)))));
      // Past the bloc's first refresh.
      await tester.pump(const Duration(milliseconds: 300));
    }

    testWidgets('offers Create ticket and no longer starts a task', (tester) async {
      await sheet(tester, ideaOf('i', 'x', projectId: 'p'));
      expect(find.byKey(const Key('idea-make-ticket')), findsOneWidget);
      expect(find.textContaining('Start as a task'), findsNothing);
    });

    testWidgets('a server without the feature gets no Create ticket', (tester) async {
      await sheet(tester, ideaOf('i', 'x', projectId: 'p'), canMakeTicket: false);
      expect(find.byKey(const Key('idea-make-ticket')), findsNothing);
    });

    testWidgets('an idea with a ticket opens it instead', (tester) async {
      await sheet(tester, ideaOf('i', 'x', projectId: 'p', ticketId: 'k', ticketNumber: 16));
      expect(find.byKey(const Key('idea-make-ticket')), findsNothing);
      expect(find.text('Open ticket #16'), findsOneWidget);
    });
  });

  group('Give work (ticket #16)', () {
    test('is a started ticket for the lead, never a task', () {
      final command = givenWorkTicket('p', 'Write the post', 'ada');
      expect(command['method'], 'ticket.create');
      final params = paramsOf(command);
      expect((params['project_id'], params['title'], params['owner_employee_id'], params['start']), ('p', 'Write the post', 'ada', true));
    });

    testWidgets('the box on Home sends it as a ticket', (tester) async {
      final repo = created();
      final company = companyWith();
      await pumpBoardPage(tester, repo, Scaffold(body: GiveWorkBox(state: company)), company: company);
      await tester.enterText(find.byKey(const ValueKey('give-work-title')), 'Write the post');
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('give-work-send')));
      await settleBoard(tester);
      final write = writesOf(repo).single;
      expect(write['method'], 'ticket.create');
      expect(paramsOf(write), containsPair('start', true));
      expect(paramsOf(write), containsPair('title', 'Write the post'));
    });
  });
}
