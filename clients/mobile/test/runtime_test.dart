import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/commands.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/data/run_config.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/company_org/employee_run_sheet.dart';
import 'package:aurelius_mobile/features/runtime/runtime_bloc.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';

class MockRepository extends Mock implements AureliusRepository {}

class MockRuntimeBloc extends MockBloc<RuntimeEvent, RuntimeState> implements RuntimeBloc {}

class MockCompanyBloc extends MockBloc<CompanyEvent, CompanyState> implements CompanyBloc {}

const projectId = '01900000-0000-7000-8000-000000000001';
const employeeId = '01900000-0000-7000-8000-000000000005';
const connectionId = '01900000-0000-7000-8000-00000000003c';

Map<String, dynamic> setting(String? value) => value == null ? {'mode': 'inherit'} : {'mode': 'value', 'value': value};

/// `connection.list` as the server answers: one Claude Code connection, global memory and
/// employee defaults, and a project override for memory.
Map<String, dynamic> fixtureJson() => {
      'as_of_seq': 95,
      'connections': [
        {
          'id': connectionId,
          'provider': 'claude_code',
          'display_name': 'Claude Code',
          'auth_kind': 'subscription',
          'billing': 'subscription',
          'status': 'ready',
          'has_secret': false,
          'default_model': 'sonnet',
          'models': ['opus', 'sonnet', 'claude-haiku-4-5'],
        },
      ],
      'defaults': <Map<String, dynamic>>[
        {
          'scope': {'kind': 'global'},
          'purpose': 'memory',
          'settings': {'connection': setting(connectionId), 'model': setting('opus'), 'persona': setting(null), 'effort': setting('high')},
        },
        {
          'scope': {'kind': 'global'},
          'purpose': 'employee',
          'settings': {'connection': setting(connectionId), 'model': setting(null), 'persona': setting(null), 'effort': setting(null)},
        },
        {
          'scope': {'kind': 'project', 'id': projectId},
          'purpose': 'memory',
          'settings': {'connection': setting(null), 'model': setting('haiku'), 'persona': setting(null), 'effort': setting(null)},
        },
      ],
    };

ConnectionsSnapshot fixtureSnapshot([Map<String, dynamic>? json]) => ConnectionsSnapshot.fromJson(json ?? fixtureJson());

RunConfig fixture() => RunConfig.of(fixtureSnapshot());

/// The fixture plus employee-purpose layers for the project and the employee.
RunConfig layered({String? projectModel, String? projectEffort, String? managerModel, String? managerEffort}) {
  final json = fixtureJson();
  (json['defaults'] as List<Map<String, dynamic>>).addAll([
    {
      'scope': {'kind': 'project', 'id': projectId},
      'purpose': 'employee',
      'settings': {'model': setting(projectModel), 'effort': setting(projectEffort)},
    },
    {
      'scope': {'kind': 'employee', 'id': employeeId},
      'purpose': 'employee',
      'settings': {'connection': setting(null), 'persona': setting(null), 'model': setting(managerModel), 'effort': setting(managerEffort)},
    },
  ]);
  return RunConfig.of(fixtureSnapshot(json));
}

/// One `defaults` row for [purpose] at the employee (or global) scope.
Map<String, dynamic> row(String purpose, {bool global = false, String? model, String? effort}) => {
      'scope': global ? {'kind': 'global'} : {'kind': 'employee', 'id': employeeId},
      'purpose': purpose,
      'settings': {'model': setting(model), 'effort': setting(effort)},
    };

/// The fixture plus extra `defaults` rows.
RunConfig withRows(List<Map<String, dynamic>> rows) {
  final json = fixtureJson();
  (json['defaults'] as List<Map<String, dynamic>>).addAll(rows);
  return RunConfig.of(fixtureSnapshot(json));
}

EmployeeSummary employee() => employeeOf(id: employeeId, name: 'Iris', role: 'Marketing');

void main() {
  test('global memory and employee defaults; project overrides are kept apart', () {
    final s = fixture();
    expect(s.defaults.keys, unorderedEquals([Purpose.memory, Purpose.employee]));
    expect(s.at(const RunScope.project(projectId), Purpose.memory)!.model, 'haiku');
    final memory = s.defaults[Purpose.memory]!;
    expect((memory.model, memory.effort), ('opus', 'high'));
    expect(s.connections.single.supportsEffort, isTrue);
    final choice = RuntimeState(loading: false, snapshot: s).choice(Purpose.employee)!;
    expect(choice.model, 'sonnet', reason: 'no explicit model: the connection default');
    expect(choice.effort, isNull);
  });

  test('changing model or effort keeps the other fields', () {
    final chat = fixture().defaults[Purpose.memory]!;
    final next = chat.withModel('sonnet').withEffort(null);
    expect(next.model, 'sonnet');
    expect(next.effort, isNull);
    expect(next.settings.effort!.toJson(), {'mode': 'inherit'});
    expect(next.connectionId, chat.connectionId);
    expect(next.settings.persona!.toJson(), chat.settings.persona!.toJson());
  });

  test('an employee resolves employee, then project, then global, then the connection', () {
    final none = fixture().employeeRun(employeeId, projectId: projectId);
    expect(none.model, const Resolved('sonnet', RunOrigin.connection));
    expect(none.effort, const Resolved<String>(null, RunOrigin.none));
    expect(none.own.scope, const RunScope.employee(employeeId));
    expect(none.connection!.displayName, 'Claude Code', reason: 'the connection from the global employee default');

    final project = layered(projectModel: 'opus', projectEffort: 'low').employeeRun(employeeId, projectId: projectId);
    expect(project.model, const Resolved('opus', RunOrigin.project));
    expect(project.effort, const Resolved('low', RunOrigin.project));

    final own = layered(projectModel: 'opus', projectEffort: 'low', managerModel: 'claude-haiku-4-5').employeeRun(employeeId, projectId: projectId);
    expect(own.model, const Resolved('claude-haiku-4-5', RunOrigin.employee));
    expect(own.effort, const Resolved('low', RunOrigin.project), reason: 'effort still inherits');
  });

  test('connection.set_defaults carries the employee scope; Inherit clears the field', () {
    final own = layered(managerModel: 'opus', managerEffort: 'max').employeeRun(employeeId, projectId: projectId).own;
    final cleared = own.withModel(null);
    final sent = Commands.setRunDefaults(Purpose.employee, cleared.settings, scope: cleared.scope);
    expect(sent['method'], 'connection.set_defaults');
    final params = paramsOf(sent);
    expect(params['scope'], {'kind': 'employee', 'id': employeeId});
    expect(params['purpose'], 'employee');
    expect((params['settings'] as Map)['model'], {'mode': 'inherit'});
    expect((params['settings'] as Map)['effort'], {'mode': 'value', 'value': 'max'});
    expect(paramsOf(Commands.setRunDefaults(Purpose.memory, const RunSettings()))['scope'], {'kind': 'global'});
  });

  test('subagent rows parse; they sit beside the employee layers', () {
    final s = withRows([row('subagent', global: true, model: 'opus'), row('subagent', effort: 'low')]);
    expect(s.defaults.keys, unorderedEquals([Purpose.memory, Purpose.employee, Purpose.subagent]));
    expect(s.defaults[Purpose.subagent]!.model, 'opus');
    expect(s.at(const RunScope.employee(employeeId), Purpose.subagent)!.effort, 'low');
    expect(s.scoped.length, 2, reason: 'the project memory row and the subagent row');
    expect(s.employeeRun(employeeId, projectId: projectId).connection!.displayName, 'Claude Code');
  });

  test("subagents keep their own model default but run on the employee's connection and effort", () {
    final none = layered(managerModel: 'opus', managerEffort: 'max').employeeRun(employeeId, projectId: projectId, purpose: Purpose.subagent);
    expect(none.model, const Resolved<String>(null, RunOrigin.none), reason: "Aurelius's Sonnet default, not the employee's model");
    expect(none.effort, const Resolved('max', RunOrigin.employee, fallback: true));
    expect(none.connection!.displayName, 'Claude Code');

    final set = withRows([row('subagent', model: 'claude-haiku-4-5', effort: 'low')]).employeeRun(employeeId, projectId: projectId, purpose: Purpose.subagent);
    expect(set.model, const Resolved('claude-haiku-4-5', RunOrigin.employee));
    expect(set.effort, const Resolved<String>(null, RunOrigin.none, fallback: true), reason: 'a stored subagent effort is not applied');
  });

  test('the global subagent default falls back to the default for all employees', () {
    final json = fixtureJson();
    ((json['defaults'] as List<Map<String, dynamic>>)[1]['settings'] as Map)['effort'] = setting('medium');
    final s = RunConfig.of(fixtureSnapshot(json));
    final run = s.globalRun(Purpose.subagent);
    expect(run.own.scope, RunScope.global);
    expect(run.effort, const Resolved('medium', RunOrigin.global, fallback: true));
    expect(s.replacing(run.own.withModel('opus')).globalRun(Purpose.subagent).model, const Resolved('opus', RunOrigin.global));
    expect(paramsOf(Commands.setRunDefaults(Purpose.subagent, const RunSettings()))['purpose'], 'subagent');
  });

  test('friendly model names match the desktop', () {
    expect(ModelCatalog.friendly('opus'), 'Opus 5.5');
    expect(ModelCatalog.friendly('claude-haiku-4-5'), 'Haiku 4.5');
    expect(ModelCatalog.friendly('gpt-5-mini'), 'GPT-5 Mini');
    expect(ModelCatalog.friendly('claude-opus-4-1'), 'Opus 4.1');
    expect(ModelCatalog.friendly('fake-deterministic-1'), 'Test runtime');
    expect(ModelCatalog.effortLabel('xhigh'), 'Extra high');
    expect(ModelCatalog.effortLabel(null), 'Default');
  });

  group('RuntimeBloc', () {
    late MockRepository repo;
    setUpAll(() => registerFallbackValue(<String, dynamic>{}));
    setUp(() {
      repo = MockRepository();
      when(() => repo.online).thenAnswer((_) => const Stream.empty());
      when(() => repo.companyChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.linkState).thenReturn(const LinkState(status: LinkStatus.online));
      when(() => repo.connections()).thenAnswer((_) async => fixtureSnapshot());
    });

    Receipt saved() => receiptOf(const CommandResultRunDefaultsSet());

    blocTest<RuntimeBloc, RuntimeState>(
      'selecting an effort sends connection.set_defaults for the global memory default',
      build: () {
        when(() => repo.write(any(), retry: false)).thenAnswer((_) async => saved());
        return RuntimeBloc(repo);
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 200));
        bloc.add(const RuntimeEffortSelected(Purpose.memory, 'max'));
      },
      wait: const Duration(milliseconds: 100),
      verify: (bloc) {
        expect(bloc.state.choice(Purpose.memory)!.effort, 'max');
        final sent = verify(() => repo.write(captureAny(), retry: false)).captured.single as Map<String, dynamic>;
        expect(sent['method'], 'connection.set_defaults');
        final params = paramsOf(sent);
        expect(params['scope'], {'kind': 'global'});
        expect(params['purpose'], 'memory');
        expect((params['settings'] as Map)['model'], {'mode': 'value', 'value': 'opus'}, reason: 'model untouched');
        expect((params['settings'] as Map)['effort'], {'mode': 'value', 'value': 'max'});
      },
    );

    blocTest<RuntimeBloc, RuntimeState>(
      'a refused change rolls back',
      build: () {
        when(() => repo.write(any(), retry: false)).thenThrow(const ProtocolException(ErrorCode.forbidden, 'This is only available in Aurelius on the PC.'));
        return RuntimeBloc(repo);
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 200));
        bloc.add(const RuntimeModelSelected(Purpose.memory, 'claude-haiku-4-5'));
      },
      wait: const Duration(milliseconds: 100),
      verify: (bloc) => expect(bloc.state.choice(Purpose.memory)!.model, 'opus'),
    );

    blocTest<RuntimeBloc, RuntimeState>(
      'an employee without its own layer gets one with only the chosen field',
      build: () {
        when(() => repo.write(any(), retry: false)).thenAnswer((_) async => saved());
        return RuntimeBloc(repo);
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 200));
        bloc.add(const RuntimeEffortSelected(Purpose.employee, 'low', scope: RunScope.employee(employeeId)));
      },
      wait: const Duration(milliseconds: 100),
      verify: (bloc) {
        expect(bloc.state.employeeRun(employeeId, projectId: projectId)!.effort, const Resolved('low', RunOrigin.employee));
        expect(bloc.state.choice(Purpose.employee)!.effort, isNull, reason: 'the global default is untouched');
        final sent = verify(() => repo.write(captureAny(), retry: false)).captured.single as Map<String, dynamic>;
        final params = paramsOf(sent);
        expect(params['scope'], {'kind': 'employee', 'id': employeeId});
        expect(params['purpose'], 'employee');
        expect(params['settings'], {
          'effort': {'mode': 'value', 'value': 'low'},
        });
      },
    );

    blocTest<RuntimeBloc, RuntimeState>(
      'a global subagent default without a row yet starts all inherit',
      build: () {
        when(() => repo.write(any(), retry: false)).thenAnswer((_) async => saved());
        return RuntimeBloc(repo);
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 200));
        bloc.add(const RuntimeModelSelected(Purpose.subagent, 'claude-haiku-4-5'));
      },
      wait: const Duration(milliseconds: 100),
      verify: (bloc) {
        expect(bloc.state.snapshot!.globalRun(Purpose.subagent).model, const Resolved('claude-haiku-4-5', RunOrigin.global));
        final sent = verify(() => repo.write(captureAny(), retry: false)).captured.single as Map<String, dynamic>;
        final params = paramsOf(sent);
        expect(params['scope'], {'kind': 'global'});
        expect(params['purpose'], 'subagent');
        expect(params['settings'], {
          'model': {'mode': 'value', 'value': 'claude-haiku-4-5'},
        });
      },
    );

    blocTest<RuntimeBloc, RuntimeState>(
      'a global row keeps the fields the phone does not touch',
      build: () {
        final json = fixtureJson();
        (json['defaults'] as List<Map<String, dynamic>>).add({
          'scope': {'kind': 'global'},
          'purpose': 'subagent',
          'settings': {'connection': setting(connectionId), 'model': setting(null), 'persona': setting(null), 'effort': setting(null)},
        });
        when(() => repo.connections()).thenAnswer((_) async => fixtureSnapshot(json));
        when(() => repo.write(any(), retry: false)).thenAnswer((_) async => saved());
        return RuntimeBloc(repo);
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 200));
        bloc.add(const RuntimeModelSelected(Purpose.subagent, 'opus'));
      },
      wait: const Duration(milliseconds: 100),
      verify: (bloc) {
        final sent = verify(() => repo.write(captureAny(), retry: false)).captured.single as Map<String, dynamic>;
        final settings = paramsOf(sent)['settings'] as Map;
        expect(settings['connection'], {'mode': 'value', 'value': connectionId});
        expect(settings['persona'], {'mode': 'inherit'});
        expect(settings['model'], {'mode': 'value', 'value': 'opus'});
      },
    );
  });

  group('Run tasks one at a time', () {
    late MockRepository repo;
    setUpAll(() => registerFallbackValue(<String, dynamic>{}));
    setUp(() {
      repo = MockRepository();
      when(() => repo.online).thenAnswer((_) => const Stream.empty());
      when(() => repo.changes).thenAnswer((_) => const Stream.empty());
      when(() => repo.linkState).thenReturn(const LinkState(status: LinkStatus.online));
      when(() => repo.projects()).thenAnswer((_) async => const []);
      when(() => repo.company()).thenAnswer((_) async => companyOf(employees: [employee()]));
      when(() => repo.proposals()).thenAnswer((_) async => const []);
      when(() => repo.tasks()).thenAnswer((_) async => const []);
      when(() => repo.write(any(), expectedVersion: any(named: 'expectedVersion'), retry: false))
          .thenAnswer((_) async => receiptOf(const CommandResultEmployeeUpdated(employeeId: employeeId, version: 2)));
    });

    test('employee.update carries sequential_tasks only when given', () {
      expect(paramsOf(Commands.updateEmployee(employeeId, 'Iris', 'Marketing')).containsKey('sequential_tasks'), isFalse);
      expect(paramsOf(Commands.updateEmployee(employeeId, 'Iris', 'Marketing', sequentialTasks: true))['sequential_tasks'], isTrue);
    });

    blocTest<CompanyBloc, CompanyState>(
      'turning it on sends employee.update with the current fields and sequential_tasks',
      build: () => CompanyBloc(repo),
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 200));
        bloc.add(const EmployeeSequentialToggled(employeeId, true));
      },
      wait: const Duration(milliseconds: 100),
      verify: (bloc) {
        final sent = verify(() => repo.write(captureAny(), expectedVersion: 1, retry: false)).captured.single as Map<String, dynamic>;
        expect(sent['method'], 'employee.update');
        final params = paramsOf(sent);
        expect((params['employee_id'], params['name'], params['role'], params['sequential_tasks']), (employeeId, 'Iris', 'Marketing', true));
        expect(params.containsKey('title'), isFalse, reason: 'the title is left as it is');
      },
    );

    blocTest<CompanyBloc, CompanyState>(
      'the same value sends nothing',
      build: () => CompanyBloc(repo),
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 200));
        bloc.add(const EmployeeSequentialToggled(employeeId, false));
      },
      wait: const Duration(milliseconds: 100),
      verify: (_) => verifyNever(() => repo.write(any(), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry'))),
    );

    blocTest<CompanyBloc, CompanyState>(
      'a refused change rolls back',
      build: () {
        when(() => repo.write(any(), expectedVersion: any(named: 'expectedVersion'), retry: false))
            .thenThrow(const ProtocolException(ErrorCode.versionConflict, 'Changed elsewhere.'));
        return CompanyBloc(repo);
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 200));
        bloc.add(const EmployeeSequentialToggled(employeeId, true));
      },
      wait: const Duration(milliseconds: 100),
      verify: (bloc) {
        expect(bloc.state.employee(employeeId)!.sequentialTasks, isFalse);
        expect(bloc.state.notice, 'Changed elsewhere.');
      },
    );
  });

  group('Employee model and effort sheet', () {
    late MockRuntimeBloc runtime;
    late MockCompanyBloc company;
    setUpAll(() {
      registerFallbackValue(const RuntimeRefreshed());
      registerFallbackValue(const CompanyRefreshed());
    });

    Future<void> pump(WidgetTester tester, RunConfig snapshot, {bool subagents = false}) async {
      tester.view.physicalSize = const Size(2400, 4800);
      addTearDown(tester.view.resetPhysicalSize);
      runtime = MockRuntimeBloc();
      whenListen(runtime, const Stream<RuntimeState>.empty(), initialState: RuntimeState(loading: false, snapshot: snapshot));
      company = MockCompanyBloc();
      whenListen(company, const Stream<CompanyState>.empty(), initialState: CompanyState(loading: false, projects: [projectOf(id: projectId, name: 'Atlas')], employees: [employee()]));
      await tester.pumpWidget(MultiBlocProvider(
        providers: [BlocProvider<RuntimeBloc>.value(value: runtime), BlocProvider<CompanyBloc>.value(value: company)],
        child: MaterialApp(
          theme: aureliusTheme(),
          home: Scaffold(
            body: Builder(builder: (context) => TextButton(onPressed: () => showEmployeeRunSheet(context, employee(), subagents: subagents), child: const Text('open'))),
          ),
        ),
      ));
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
    }

    testWidgets('rows show the effective value and where it comes from; no project layer outside a task', (tester) async {
      await pump(tester, layered(projectEffort: 'high', managerModel: 'opus'));
      final model = find.byKey(const ValueKey('employee-run-model'));
      expect(find.descendant(of: model, matching: find.text('Opus 5.5')), findsOneWidget);
      expect(find.descendant(of: model, matching: find.text('Set for Iris')), findsOneWidget);
      final effort = find.byKey(const ValueKey('employee-run-effort'));
      expect(find.descendant(of: effort, matching: find.text('High')), findsNothing, reason: 'employees work company-wide');
      expect(find.descendant(of: effort, matching: find.text('From project Atlas')), findsNothing);
    });

    testWidgets('Inherit shows what it gives and clears the employee layer', (tester) async {
      await pump(tester, layered(managerModel: 'opus'));
      await tester.tap(find.byKey(const ValueKey('employee-run-model')));
      await tester.pumpAndSettle();
      expect(find.text("Sonnet 5.5 · The connection's default"), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('employee-run-inherit')));
      await tester.pumpAndSettle();
      final event = verify(() => runtime.add(captureAny())).captured.single as RuntimeModelSelected;
      expect((event.purpose, event.model, event.scope), (Purpose.employee, null, const RunScope.employee(employeeId)));
    });

    testWidgets('choosing a model sends it for this employee', (tester) async {
      await pump(tester, fixture());
      await tester.tap(find.byKey(const ValueKey('employee-run-model')));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Haiku 4.5'));
      await tester.pumpAndSettle();
      final event = verify(() => runtime.add(captureAny())).captured.single as RuntimeModelSelected;
      expect((event.model, event.scope), ('claude-haiku-4-5', const RunScope.employee(employeeId)));
    });

    testWidgets("the toggle asks to run this employee's tasks one at a time", (tester) async {
      company = MockCompanyBloc();
      whenListen(company, const Stream<CompanyState>.empty(), initialState: CompanyState(loading: false, employees: [employee()]));
      await tester.pumpWidget(BlocProvider<CompanyBloc>.value(
        value: company,
        child: MaterialApp(theme: aureliusTheme(), home: Scaffold(body: EmployeeSequentialTile(employee: employee()))),
      ));
      final tile = find.byKey(const ValueKey('employee-sequential'));
      expect(find.descendant(of: tile, matching: find.text('Tasks wait in line and start one after the other.')), findsOneWidget);
      await tester.tap(tile);
      await tester.pumpAndSettle();
      final event = verify(() => company.add(captureAny())).captured.single as EmployeeSequentialToggled;
      expect((event.employeeId, event.value), (employeeId, true));
    });

    testWidgets('there is no section for tasks given to the employee any more; subagents show their values and origins', (tester) async {
      await pump(tester, layered(managerModel: 'opus', managerEffort: 'max'));
      expect(find.text('TASKS GIVEN TO THIS EMPLOYEE'), findsNothing);
      expect(find.byKey(const ValueKey('worker-run-model')), findsNothing);
      expect(find.byKey(const ValueKey('subagent-run-model')), findsNothing, reason: "subagents sit under Edit's Advanced");
      await tester.tapAt(const Offset(10, 10));
      await tester.pumpAndSettle();
      await pump(tester, layered(managerModel: 'opus', managerEffort: 'max'), subagents: true);
      expect(find.byKey(const ValueKey('employee-run-model')), findsNothing);
      final subagent = find.byKey(const ValueKey('subagent-run-model'));
      expect(find.descendant(of: subagent, matching: find.text('Default')), findsOneWidget);
      expect(find.descendant(of: subagent, matching: find.text("Aurelius's default for subagents (Sonnet)")), findsOneWidget);
      final effort = find.byKey(const ValueKey('subagent-run-effort'));
      expect(find.descendant(of: effort, matching: find.text('Effort: follows the employee')), findsOneWidget);
      await tester.tap(effort);
      await tester.pumpAndSettle();
      expect(find.text("Effort for Iris's subagents"), findsNothing, reason: 'Claude Code takes no subagent effort: no picker');
    });
  });
}
