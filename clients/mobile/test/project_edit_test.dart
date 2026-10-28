import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/protocol/models.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:aurelius_mobile/features/project/project_page.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';

class MockRepository extends Mock implements AureliusRepository {}

class MockConnectionBloc extends MockBloc<ConnectionEvent, LinkState> implements ConnectionBloc {}

LinkState linkWith(List<String> features) =>
    LinkState(status: LinkStatus.online, capabilities: const ['read', 'chat', 'task_control', 'approve'], features: features);

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  late MockRepository repo;

  setUp(() {
    repo = MockRepository();
    when(() => repo.host).thenReturn(const Host(hostId: 'h', name: 'desktop', url: 'http://x', clientId: 'c', environment: 'dev', pairedAtMs: 0));
    when(() => repo.online).thenAnswer((_) => const Stream.empty());
    when(() => repo.changes).thenAnswer((_) => const Stream.empty());
    when(() => repo.companyChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.projects()).thenAnswer((_) async => [projectOf(id: 'p', name: 'Launch', description: 'Ship it')]);
    when(() => repo.tasks()).thenAnswer((_) async => []);
    when(() => repo.tasks(projectId: any(named: 'projectId'))).thenAnswer((_) async => []);
    when(() => repo.ideas(any())).thenAnswer((_) async => []);
    when(() => repo.company()).thenAnswer((_) async => companyOf(employees: []));
    when(() => repo.proposals()).thenAnswer((_) async => []);
    when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
        .thenAnswer((_) async => receiptOf(const CommandResultProjectRenamed(projectId: 'p', version: 2)));
  });

  /// The project page with its menu open on Edit….
  Future<void> openEdit(WidgetTester tester, List<String> features) async {
    final connection = MockConnectionBloc();
    whenListen(connection, const Stream<LinkState>.empty(), initialState: linkWith(features));
    when(() => repo.linkState).thenReturn(linkWith(features));
    tester.view.physicalSize = const Size(1200, 3000);
    addTearDown(tester.view.resetPhysicalSize);
    await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
      value: repo,
      child: MultiBlocProvider(
        providers: [
          BlocProvider<ConnectionBloc>.value(value: connection),
          BlocProvider<CompanyBloc>(lazy: false, create: (_) => CompanyBloc(repo)),
        ],
        child: MaterialApp(theme: aureliusTheme(), home: const ProjectPage(projectId: 'p')),
      ),
    ));
    await tester.pump(const Duration(milliseconds: 500));
    await tester.tap(find.byTooltip('Project actions'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Edit…'));
    await tester.pumpAndSettle();
  }

  testWidgets('Edit… is one form with name and folder; the description is left out on older servers', (tester) async {
    await openEdit(tester, const []);
    expect(find.widgetWithText(TextField, 'Name'), findsOneWidget);
    expect(find.widgetWithText(TextField, 'Description (optional)'), findsNothing);
    expect(find.widgetWithText(TextField, 'Folder on the PC (optional)'), findsOneWidget);
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
  });

  testWidgets('saving without a change sends nothing', (tester) async {
    await openEdit(tester, const ['project_description']);
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
  });

  testWidgets('project_description: the description is saved through project.set_description with the version', (tester) async {
    await openEdit(tester, const ['project_description']);
    final field = tester.widget<TextField>(find.widgetWithText(TextField, 'Description (optional)'));
    expect(field.maxLength, 2000);
    expect(field.controller!.text, 'Ship it');

    await tester.enterText(find.widgetWithText(TextField, 'Description (optional)'), '  Ship it by Friday\nwith docs  ');
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    final request = verify(() => repo.write(captureAny(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: 1, retry: true)).captured.single as Map<String, dynamic>;
    expect(request['method'], 'project.set_description');
    expect(request['params'], {'description': 'Ship it by Friday\nwith docs', 'project_id': 'p'});
    expect(find.text('Saved.'), findsOneWidget);
  });

  testWidgets('an empty description clears it', (tester) async {
    await openEdit(tester, const ['project_description']);
    await tester.enterText(find.widgetWithText(TextField, 'Description (optional)'), '');
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    final request = writesOf(repo).single;
    expect(request['method'], 'project.set_description');
    expect(request['params'], {'description': '', 'project_id': 'p'});
  });

  testWidgets('name and folder go as separate commands, the first with the version', (tester) async {
    await openEdit(tester, const []);
    await tester.enterText(find.widgetWithText(TextField, 'Name'), 'Launch 2');
    await tester.enterText(find.widgetWithText(TextField, 'Folder on the PC (optional)'), r'C:\Work\launch');
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    final all = writesOf(repo);
    expect(all.map((r) => r['method']), ['project.rename', 'project.set_folder']);
    expect(all.last['params'], {'path': r'C:\Work\launch', 'project_id': 'p'});
  });

  testWidgets('a server error is shown', (tester) async {
    when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
        .thenThrow(const ProtocolException(ErrorCode.invalidRequest, 'description is longer than 2000 characters'));
    await openEdit(tester, const ['project_description']);
    await tester.enterText(find.widgetWithText(TextField, 'Description (optional)'), 'New');
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(find.text('description is longer than 2000 characters'), findsOneWidget);
  });
}
