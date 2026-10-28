import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/commands.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/ideas/ideas_bloc.dart';
import 'package:aurelius_mobile/features/ideas/start_mode_sheet.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

class MockRepository extends Mock implements AureliusRepository {}

void main() {
  test('idea.start sends run.mode only when a mode is chosen', () {
    expect(Commands.startIdea('i1', 'm1'), {'method': 'idea.start', 'params': {'idea_id': 'i1', 'employee_id': 'm1'}});
    expect((Commands.startIdea('i1', 'm1', mode: AgentMode.acceptEdits)['params'] as Map)['run'], {'mode': 'accept_edits'});
    expect((Commands.startIdea('i1', 'm1', mode: AgentMode.auto)['params'] as Map)['run'], {'mode': 'auto'});
  });

  test('Auto needs the Approve right', () {
    final repo = MockRepository();
    when(() => repo.online).thenAnswer((_) => const Stream.empty());
    when(() => repo.companyChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.linkState).thenReturn(const LinkState(status: LinkStatus.connecting, capabilities: ['read', 'task_control']));
    final bloc = IdeasBloc(repo, 'p1');
    expect(bloc.canStartInAuto, false);
    when(() => repo.linkState).thenReturn(const LinkState(status: LinkStatus.connecting, capabilities: ['read', 'approve']));
    expect(bloc.canStartInAuto, true);
    bloc.close();
  });

  Future<List<StartMode?>> open(WidgetTester tester, {required bool canAuto}) async {
    final picked = <StartMode?>[];
    await tester.pumpWidget(MaterialApp(
      theme: aureliusTheme(),
      home: Builder(
        builder: (context) => Scaffold(
          body: TextButton(
            onPressed: () async => picked.add(await pickStartMode(context, employeeName: 'Max', canAuto: canAuto)),
            child: const Text('start'),
          ),
        ),
      ),
    ));
    await tester.tap(find.text('start'));
    await tester.pumpAndSettle();
    return picked;
  }

  testWidgets("Employee's default is chosen first and gives no mode", (tester) async {
    final picked = await open(tester, canAuto: true);
    expect(find.text("Employee's default"), findsOneWidget);
    for (final label in ['Ask', 'Accept edits', 'Plan', 'Auto']) {
      expect(find.text(label), findsOneWidget);
    }
    final first = tester.widget<ListTile>(find.byKey(const Key('start-mode-default')));
    expect((first.leading as Icon).icon, Icons.radio_button_checked);
    await tester.tap(find.byKey(const Key('start-mode-default')));
    await tester.pumpAndSettle();
    expect(picked.single, (mode: null, variants: 1));
  });

  testWidgets('a chosen mode is returned', (tester) async {
    final picked = await open(tester, canAuto: true);
    await tester.tap(find.byKey(const Key('start-mode-plan')));
    await tester.pumpAndSettle();
    expect(picked.single, (mode: AgentMode.plan, variants: 1));
  });

  testWidgets('without the Approve right Auto is off and says why', (tester) async {
    final picked = await open(tester, canAuto: false);
    expect(tester.widget<ListTile>(find.byKey(const Key('start-mode-auto'))).enabled, isFalse);
    expect(find.textContaining('Needs the Approve right'), findsOneWidget);
    await tester.tap(find.byKey(const Key('start-mode-auto')));
    await tester.pumpAndSettle();
    expect(picked, isEmpty);
  });
}
