import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:mocktail/mocktail.dart';

import 'builders.dart';

// Builders and a harness for the Board's tests: tickets, columns, a mocked
// repository that answers the board reads, and a router with the pages a
// ticket opens.

class MockRepository extends Mock implements AureliusRepository {}

class MockCompanyBloc extends MockBloc<CompanyEvent, CompanyState> implements CompanyBloc {}

class MockConnectionBloc extends MockBloc<ConnectionEvent, LinkState> implements ConnectionBloc {}

const testUser = UserView(id: 'u-1', username: 'ada', createdAtMs: 0);
const onlineLink = LinkState(status: LinkStatus.online, capabilities: ['read', 'chat', 'task_control', 'approve'], boardMembersExist: true, user: testUser);
const offlineLink = LinkState(status: LinkStatus.offline, boardMembersExist: true, user: testUser);

/// The six columns in their default order (MU1-05 AC1).
const defaultColumns = [
  (ColumnKind.backlog, 'Backlog'),
  (ColumnKind.todo, 'Todo'),
  (ColumnKind.doing, 'Doing'),
  (ColumnKind.planReview, 'Plan review'),
  (ColumnKind.review, 'Review'),
  (ColumnKind.done, 'Done'),
];

String columnIdOf(ColumnKind kind) => 'col-${kind.wire}';

BoardColumnView columnOf(ColumnKind kind, {String? name, int position = 0, int version = 1}) =>
    BoardColumnView(id: columnIdOf(kind), kind: kind, name: name ?? defaultColumns.firstWhere((c) => c.$1 == kind).$2, position: position, version: version);

TicketView ticketOf({
  String id = 'k1',
  int number = 1,
  String title = 'Write the release notes',
  ColumnKind column = ColumnKind.backlog,
  String projectId = 'p1',
  String projectName = 'Atlas',
  TicketPriority priority = TicketPriority.normal,
  String? ownerEmployeeId,
  String? ownerName,
  String? rootTaskId,
  TaskStatus? rootTaskStatus,
  List<String> labels = const [],
  int? deadlineMs,
  int? doneAtMs,
  String? description,
  String? reviewFeedback,
  TaskRun run = const TaskRun(),
  double position = 1,
  int version = 3,
}) =>
    TicketView(
      attachments: const [],
      column: column,
      columnId: columnIdOf(column),
      createdAtMs: 0,
      createdByUser: testUser.id,
      createdByUsername: testUser.username,
      deadlineMs: deadlineMs,
      description: description,
      doneAtMs: doneAtMs,
      id: id,
      labels: labels,
      number: number,
      ownerEmployeeId: ownerEmployeeId,
      ownerName: ownerName,
      position: position,
      priority: priority,
      projectId: projectId,
      projectName: projectName,
      reviewFeedback: reviewFeedback,
      rootTaskId: rootTaskId,
      rootTaskStatus: rootTaskStatus,
      run: run,
      title: title,
      updatedAtMs: 0,
      version: version,
    );

/// A board with [tickets] in their columns (in the given order) and the six
/// columns at [versions] (by kind; 1 when left out).
BoardView boardOf(List<TicketView> tickets, {String? projectId, Map<ColumnKind, String> names = const {}, Map<ColumnKind, int> versions = const {}}) => BoardView(
      asOfSeq: 1,
      projectId: projectId,
      columns: [
        for (final (i, (kind, _)) in defaultColumns.indexed)
          BoardColumnCards(column: columnOf(kind, name: names[kind], position: i, version: versions[kind] ?? 1), tickets: [for (final t in tickets) if (t.column == kind) t]),
      ],
    );

TimelineCard cardOf(String type, {String actor = 'user', int seq = 1}) =>
    TimelineCard(actor: actor, atMs: 0, correlationId: 'k1', detail: 'ticket', kind: 'activity', label: 'ACTIVITY', seq: seq, title: type);

final orion = employeeOf(id: 'orion', name: 'Orion', role: 'Project Manager');
final forge = employeeOf(id: 'forge', name: 'Forge', role: 'Backend Developer', reportsTo: 'orion');

/// A company with Orion (the company's dispatcher unless [dispatcher] says
/// otherwise) and Forge, one project and [tasks].
CompanyState companyWith({String? dispatcher = 'orion', String? projectDispatcher, List<TaskView> tasks = const [], List<EmployeeSummary>? employees, List<ProjectSummary>? projects}) => CompanyState(
      loading: false,
      company: CompanyView(id: 'c', name: 'Aurelius', version: 4, dispatcherEmployeeId: dispatcher),
      employees: employees ?? [orion, forge],
      projects: projects ?? [ProjectSummary(createdAtMs: 0, id: 'p1', name: 'Atlas', status: ProjectStatus.active, version: 2, employeesActive: 0, dispatcherEmployeeId: projectDispatcher)],
      tasks: tasks,
    );

/// A repository that answers the board reads; every write succeeds.
MockRepository boardRepository({BoardView? board, TicketDetail? detail, List<TimelineCard> activity = const [], MessagesSnapshot? messages, List<JobSummary> jobs = const []}) {
  final repo = MockRepository();
  when(() => repo.host).thenReturn(const Host(hostId: 'h', name: 'desktop', url: 'http://x', clientId: 'c', environment: 'dev', pairedAtMs: 0));
  when(() => repo.linkState).thenReturn(onlineLink);
  when(() => repo.online).thenAnswer((_) => const Stream.empty());
  when(() => repo.changes).thenAnswer((_) => const Stream.empty());
  when(() => repo.boardChanges).thenAnswer((_) => const Stream.empty());
  when(() => repo.board(projectId: any(named: 'projectId'))).thenAnswer((_) async => board ?? boardOf(const []));
  if (detail != null) when(() => repo.ticket(any())).thenAnswer((_) async => detail);
  when(() => repo.timeline(correlationId: any(named: 'correlationId'), activity: any(named: 'activity'), limit: any(named: 'limit')))
      .thenAnswer((_) async => TimelinePage(cards: activity));
  if (messages != null) when(() => repo.messages(any())).thenAnswer((_) async => messages);
  when(() => repo.jobs()).thenAnswer((_) async => jobs);
  when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
      .thenAnswer((_) async => receiptOf(const CommandResultTicketChanged(column: ColumnKind.todo, ticketId: 'k1', version: 4)));
  return repo;
}

/// [page] under a router that also knows the pages a ticket leads to (as text).
Future<GoRouter> pumpBoardPage(WidgetTester tester, MockRepository repo, Widget page, {CompanyState? company, MockCompanyBloc? companyBloc, LinkState link = onlineLink, Size size = const Size(390, 844)}) async {
  tester.view.physicalSize = size * 3;
  tester.view.devicePixelRatio = 3;
  addTearDown(tester.view.reset);
  companyBloc ??= MockCompanyBloc();
  whenListen(companyBloc, const Stream<CompanyState>.empty(), initialState: company ?? companyWith());
  final connection = MockConnectionBloc();
  whenListen(connection, const Stream<LinkState>.empty(), initialState: link);
  final router = GoRouter(initialLocation: '/', routes: [
    GoRoute(path: '/', builder: (_, _) => page),
    GoRoute(path: '/ticket/:tid', builder: (_, s) => Scaffold(body: Text('ticket ${s.pathParameters['tid']}'))),
    GoRoute(path: '/ticket/:tid/plan', builder: (_, s) => Scaffold(body: Text('plan ${s.pathParameters['tid']}'))),
    GoRoute(path: '/board/new', builder: (_, s) => Scaffold(body: Text('new ticket ${s.uri.queryParameters['project'] ?? ''}'))),
    GoRoute(path: '/settings/board', builder: (_, _) => const Scaffold(body: Text('board settings'))),
    GoRoute(path: '/project/:pid', builder: (_, s) => Scaffold(body: Text('project ${s.pathParameters['pid']}'))),
    GoRoute(path: '/project/:pid/task/:tid', builder: (_, s) => Scaffold(body: Text('task ${s.pathParameters['tid']}'))),
    GoRoute(path: '/conversation/:cid', builder: (_, s) => Scaffold(body: Text('conversation ${s.pathParameters['cid']}'))),
    GoRoute(path: '/projects', builder: (_, _) => const Scaffold(body: Text('projects'))),
    GoRoute(path: '/updates', builder: (_, _) => const Scaffold(body: Text('updates'))),
  ]);
  addTearDown(router.dispose);
  await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
    value: repo,
    child: MultiBlocProvider(
      providers: [BlocProvider<CompanyBloc>.value(value: companyBloc), BlocProvider<ConnectionBloc>.value(value: connection)],
      child: MaterialApp.router(theme: aureliusTheme(), routerConfig: router),
    ),
  ));
  // The board reads after a debounce; working cards keep breathing, so step instead of settling.
  await settleBoard(tester);
  return router;
}

/// Step time instead of settling: working cards keep their dot breathing.
Future<void> settleBoard(WidgetTester tester) async {
  for (var i = 0; i < 8; i++) {
    await tester.pump(const Duration(milliseconds: 100));
  }
}
