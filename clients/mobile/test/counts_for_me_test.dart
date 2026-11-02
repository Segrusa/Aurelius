import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/tickets.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/carplay/carplay_selection.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/inbox/inbox_bloc.dart';
import 'package:aurelius_mobile/features/inbox/inbox_sections.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

// Freeze §5: Dashboard, Waiting tasks and CarPlay count a ticket's main task only
// when it needs me; tasks without a ticket and approvals stay as they are.

const _bo = UserView(id: 'u-2', username: 'bo', createdAtMs: 0);

TaskView _main(String id, String ticket) => taskOf(id: id, projectId: 'p', title: id, status: TaskStatus.waiting, waitingReason: 'input', ticketId: ticket);

TaskView _loose(String id) => taskOf(id: id, projectId: 'p', title: id, status: TaskStatus.waiting, waitingReason: 'input');

void main() {
  final others = ticketOf(id: 'k-others', number: 1, column: ColumnKind.doing, responsible: _bo);
  final mine = ticketOf(id: 'k-mine', number: 2, column: ColumnKind.doing, responsible: testUser);
  final noOne = ticketOf(id: 'k-none', number: 3, column: ColumnKind.doing);
  final tasks = [_main('t-others', 'k-others'), _main('t-mine', 'k-mine'), _main('t-none', 'k-none'), _main('t-unknown', 'k-gone'), _loose('t-loose')];

  group('othersMainTasks', () {
    test('holds only a main task whose decision is another Responsible\'s', () {
      expect(othersMainTasks(tasks, [others, mine, noOne], me: testUser.id), {'t-others'});
    });

    test('a task under a ticket that is not its main task still counts', () {
      final sub = taskOf(id: 't-sub', projectId: 'p', title: 's', status: TaskStatus.waiting, waitingReason: 'input', ticketId: 'k-others', parentTaskId: 't-others');
      expect(othersMainTasks([sub], [others], me: testUser.id), isEmpty);
    });

    test('a disabled Responsible leaves it to everyone', () {
      final disabled = ticketOf(id: 'k-others', number: 1, column: ColumnKind.doing, responsible: _bo, responsibleDisabled: true);
      expect(othersMainTasks(tasks, [disabled], me: testUser.id), isEmpty);
    });
  });

  group('what counts', () {
    final state = CompanyState(loading: false, tasks: tasks, othersTasks: const {'t-others'});

    test('the Dashboard leaves out the main task of another Responsible', () {
      expect(state.waitingTasks.map((t) => t.id), ['t-mine', 't-none', 't-unknown', 't-loose']);
      expect(state.waiting, 4);
    });

    test('Waiting tasks in the Inbox do too, and it is not listed as running', () {
      final s = InboxSections.of(const InboxState(), state);
      expect(s.waitingTasks.map((t) => t.id), isNot(contains('t-others')));
      expect(s.runningTasks.map((t) => t.id), isNot(contains('t-others')));
      expect(s.needsYou, 4);
    });

    test('an approval keeps counting whoever is responsible', () {
      final s = InboxSections.of(InboxState(approvals: [approvalOf('a1')]), state);
      expect(s.approvals, hasLength(1));
      expect(s.needsYou, 5);
    });

    test('CarPlay as well', () {
      expect(needsYouTasks(tasks, others: const {'t-others'}).map((t) => t.id), isNot(contains('t-others')));
      expect(needsYouTasks(tasks), hasLength(5), reason: 'as before without a Responsible');
    });
  });

  group('the company reads it', () {
    setUpAll(() => registerFallbackValue(<String, dynamic>{}));

    MockRepository repoWith(LinkState link, List<TaskView> tasks) {
      final repo = boardRepository(board: boardOf([others, mine, noOne]), link: link);
      when(() => repo.projects()).thenAnswer((_) async => []);
      when(() => repo.company()).thenAnswer((_) async => companyOf());
      when(() => repo.tasks()).thenAnswer((_) async => tasks);
      when(() => repo.proposals()).thenAnswer((_) async => []);
      return repo;
    }

    test('from the board, on a server with a Responsible', () async {
      final bloc = CompanyBloc(repoWith(phaseALink, tasks));
      addTearDown(bloc.close);
      final s = await bloc.stream.firstWhere((s) => !s.loading);
      expect(s.othersTasks, {'t-others'});
      expect(s.waiting, 4);
    });

    test('an older server gets no extra read and counts as now', () async {
      final repo = repoWith(onlineLink, tasks);
      final bloc = CompanyBloc(repo);
      addTearDown(bloc.close);
      final s = await bloc.stream.firstWhere((s) => !s.loading);
      verifyNever(() => repo.board(projectId: any(named: 'projectId')));
      expect(s.waiting, 5);
    });

    test('no board read while no main task waits', () async {
      final repo = repoWith(phaseALink, [_loose('t-loose')]);
      final bloc = CompanyBloc(repo);
      addTearDown(bloc.close);
      await bloc.stream.firstWhere((s) => !s.loading);
      verifyNever(() => repo.board(projectId: any(named: 'projectId')));
    });

    test('a failed board read counts them as before, it hides nothing', () async {
      final repo = repoWith(phaseALink, tasks);
      when(() => repo.board(projectId: any(named: 'projectId'))).thenThrow(const DisconnectedException());
      final bloc = CompanyBloc(repo);
      addTearDown(bloc.close);
      final s = await bloc.stream.firstWhere((s) => !s.loading);
      expect(s.waiting, 5);
    });
  });
}
