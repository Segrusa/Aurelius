import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/plugins/plugins_page.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';

class MockRepository extends Mock implements AureliusRepository {}

class MockCompanyBloc extends MockBloc<CompanyEvent, CompanyState> implements CompanyBloc {}

void main() {
  testWidgets('the Plugins page shows existing project assignments next to the employees, labelled Project', (tester) async {
    final repo = MockRepository();
    when(() => repo.linkState).thenReturn(const LinkState(status: LinkStatus.online, capabilities: ['read']));
    when(() => repo.pluginsList()).thenAnswer((_) async => PluginsSnapshot(plugins: [
          PluginInfo.fromJson({
            'id': 'ponytail@ponytail',
            'name': 'ponytail',
            'marketplace': 'ponytail',
            'version': '1',
            'description': '',
            'components': {'skills': [], 'commands': [], 'agents': [], 'hooks': false, 'mcp_servers': []},
            'employees': ['ada'],
            'projects': ['p'],
            'missing': false,
          }),
        ]));
    final company = MockCompanyBloc();
    whenListen(
      company,
      const Stream<CompanyState>.empty(),
      initialState: CompanyState(loading: false, projects: [projectOf(id: 'p', name: 'Launch')], employees: [employeeOf(id: 'ada', name: 'Ada')]),
    );
    await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
      value: repo,
      child: BlocProvider<CompanyBloc>.value(value: company, child: MaterialApp(theme: aureliusTheme(), home: const PluginsPage())),
    ));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('plugin-used-ponytail@ponytail')), findsOneWidget);
    expect(find.text('Used by Ada, Project Launch'), findsOneWidget);
  });

  test('PluginInfo parses components and employees', () {
    final p = PluginInfo.fromJson({
      'id': 'ponytail@ponytail',
      'name': 'ponytail',
      'marketplace': 'ponytail',
      'version': '5.1.0',
      'description': 'Terse',
      'components': {'skills': ['a', 'b'], 'commands': [], 'agents': [], 'hooks': true, 'mcp_servers': ['x']},
      'employees': ['m1'],
      'missing': false,
    });
    expect(p.components.chips, ['2 skills', 'hooks', 'MCP']);
    expect(p.employees, ['m1']);
    expect(p.components.hooks, isTrue);
  });

  test('offers keep unknown components as null', () {
    final c = PluginPreview.fromJson({
      'marketplace': 'm',
      'offers': [
        {'id': 'a@m', 'name': 'a', 'marketplace': 'm', 'description': '', 'installed': true},
      ],
    });
    expect(c.offers.single.components, isNull);
    expect(c.offers.single.installed, isTrue);
  });

  test('EmployeePlugins lists what is enabled and what is gone', () {
    final m = EmployeePlugins.fromJson({'employee_id': 'x', 'enabled': ['a@m'], 'missing': ['b@m']});
    expect(m.enabled, ['a@m']);
    expect(m.missing, ['b@m']);
  });
}
