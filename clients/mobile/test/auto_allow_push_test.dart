import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/push/push_action.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/commands.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/more/auto_allow_section.dart';
import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';

class MockRepository extends Mock implements AureliusRepository {}

const tools = ['Read', 'Grep', 'Glob', 'WebSearch', 'list_windows'];
const approver = LinkState(status: LinkStatus.online, capabilities: ['read', 'approve'], features: ['auto_allow']);
const reader = LinkState(status: LinkStatus.online, capabilities: ['read'], features: ['auto_allow']);
const oldServer = LinkState(status: LinkStatus.online, capabilities: ['read', 'approve']);

Receipt decided() => receiptOf(const CommandResultApprovalDecided(approvalId: 'a1', ok: true, output: '', status: ApprovalStatus.consumed));

void main() {
  setUpAll(() {
    registerFallbackValue(<String, dynamic>{});
    registerFallbackValue(Duration.zero);
  });

  group('auto-allow setting', () {
    Future<MockRepository> pump(WidgetTester tester, LinkState link) async {
      final repo = MockRepository();
      when(() => repo.linkState).thenReturn(link);
      when(() => repo.autoAllowSetting(any())).thenAnswer((_) async => const AutoAllowSetting(enabled: true, tools: tools));
      when(() => repo.setAutoAllowEnabled(any())).thenAnswer((i) async => AutoAllowSetting(enabled: i.positionalArguments.first as bool, tools: tools));
      await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
        value: repo,
        child: MaterialApp(theme: aureliusTheme(), home: const Scaffold(body: SingleChildScrollView(child: AutoAllowSection()))),
      ));
      await tester.pump();
      return repo;
    }

    testWidgets('shows the tools and the explanation, and the toggle turns it off', (tester) async {
      final repo = await pump(tester, approver);
      expect(find.text('Auto-allow safe tools'), findsOneWidget);
      expect(find.text(tools.join(', ')), findsOneWidget);
      expect(find.textContaining('WebFetch and screenshots still ask'), findsOneWidget);
      expect(tester.widget<CupertinoSwitch>(find.byType(CupertinoSwitch)).value, isTrue);
      await tester.tap(find.byType(CupertinoSwitch));
      await tester.pump();
      verify(() => repo.setAutoAllowEnabled(false)).called(1);
      expect(tester.widget<CupertinoSwitch>(find.byType(CupertinoSwitch)).value, isFalse);
    });

    testWidgets('without the Approve right it is read-only', (tester) async {
      await pump(tester, reader);
      expect(tester.widget<CupertinoSwitch>(find.byType(CupertinoSwitch)).onChanged, isNull);
      expect(find.text(autoAllowNeedsApprove), findsOneWidget);
    });

    testWidgets('servers without the feature show nothing', (tester) async {
      final repo = await pump(tester, oldServer);
      expect(find.byKey(const ValueKey('auto-allow-section')), findsNothing);
      verifyNever(() => repo.autoAllowSetting(any()));
    });
  });

  group('approve from a push action', () {
    late MockRepository repo;
    setUp(() => repo = MockRepository());

    test('a plan is approved with edits', () async {
      when(() => repo.approvals()).thenAnswer((_) async => [approvalOf('a1', tool: 'claude.ExitPlanMode', effect: 'read', arguments: {'plan': '# P'})]);
      when(() => repo.write(any(), retry: false)).thenAnswer((_) async => decided());
      final result = await approveFromPush(repo, 'a1');
      expect(result.done, isTrue);
      expect(verify(() => repo.write(captureAny(), retry: false)).captured.single, Commands.decidePlan('a1', PlanDecision.approveEdits));
    });

    test('a normal approval is approved with its digest', () async {
      when(() => repo.approvals()).thenAnswer((_) async => [approvalOf('a1')]);
      when(() => repo.write(any(), retry: false)).thenAnswer((_) async => decided());
      final result = await approveFromPush(repo, 'a1');
      expect(result.done, isTrue);
      expect(verify(() => repo.write(captureAny(), retry: false)).captured.single, Commands.decideApproval('a1', 'digest', true));
    });

    test('an unknown approval or a failure decides nothing and says why', () async {
      when(() => repo.approvals()).thenAnswer((_) async => []);
      var result = await approveFromPush(repo, 'a1');
      expect(result.done, isFalse);
      verifyNever(() => repo.write(any(), retry: any(named: 'retry')));

      when(() => repo.approvals()).thenAnswer((_) async => [approvalOf('a1')]);
      when(() => repo.write(any(), retry: false)).thenThrow(const NotSentException('k'));
      result = await approveFromPush(repo, 'a1');
      expect(result.done, isFalse);
      expect(result.message, contains('niet bereikbaar'));
    });
  });
}
