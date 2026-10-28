import 'dart:async';

import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:aurelius_mobile/features/inbox/approval_page.dart';
import 'package:aurelius_mobile/features/inbox/inbox_bloc.dart';
import 'package:aurelius_mobile/features/more/mcp_section.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';

class MockRepository extends Mock implements AureliusRepository {}

class MockConnectionBloc extends MockBloc<ConnectionEvent, LinkState> implements ConnectionBloc {}

class MockInboxBloc extends MockBloc<InboxEvent, InboxState> implements InboxBloc {}

const approver = LinkState(status: LinkStatus.online, capabilities: ['read', 'approve']);
const reader = LinkState(status: LinkStatus.online, capabilities: ['read']);

const chromeTool = 'claude.mcp__claude-in-chrome__navigate';

Map<String, dynamic> params(Map<String, dynamic> request) => request['params'] as Map<String, dynamic>;

Event eventOf(EventKind kind, String aggregateId) => Event(aggregateId: aggregateId, atMs: 0, kind: kind, seq: 11);

void main() {
  setUpAll(() {
    registerFallbackValue(<String, dynamic>{});
    registerFallbackValue(const InboxRefreshed());
  });

  test('MCP tool names read as server › tool', () {
    expect(mcpToolName('mcp__github__create_issue'), 'github › create_issue');
    expect(mcpToolName(chromeTool), 'claude-in-chrome › navigate');
    expect(mcpToolName('mcp__srv__a__b'), 'srv › a__b');
    expect(mcpToolName('claude.Bash'), 'claude.Bash');
  });

  group('approval page', () {
    Future<MockInboxBloc> pump(WidgetTester tester, ApprovalView approval, LinkState link) async {
      final connection = MockConnectionBloc();
      whenListen(connection, const Stream<LinkState>.empty(), initialState: link);
      final inbox = MockInboxBloc();
      whenListen(inbox, const Stream<InboxState>.empty(), initialState: InboxState(loading: false, approvals: [approval]));
      final repo = MockRepository();
      when(() => repo.host).thenReturn(const Host(hostId: 'h', name: 'desktop', url: 'http://x', clientId: 'c', environment: 'dev', pairedAtMs: 0));
      tester.view.physicalSize = const Size(1200, 2400);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
        value: repo,
        child: MultiBlocProvider(
          providers: [
            BlocProvider<ConnectionBloc>.value(value: connection),
            BlocProvider<InboxBloc>.value(value: inbox),
          ],
          child: MaterialApp(theme: aureliusTheme(), home: ApprovalPage(approvalId: approval.id)),
        ),
      ));
      return inbox;
    }

    testWidgets('an eligible MCP tool offers Always allow with what it means', (tester) async {
      final inbox = await pump(tester, approvalOf('a1', tool: chromeTool, alwaysAllowEligible: true), approver);
      expect(find.byKey(const ValueKey('approval-always-allow')), findsOneWidget);
      expect(find.textContaining('claude-in-chrome › navigate run without asking'), findsOneWidget);
      expect(find.textContaining('Plan mode still asks'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('approval-always-allow')));
      final event = verify(() => inbox.add(captureAny())).captured.single as ApprovalDecided;
      expect(event.approve, isTrue);
      expect(event.alwaysAllow, isTrue);
      expect(tester.takeException(), isNull);
    });

    testWidgets('no Always allow for tools the server does not mark eligible', (tester) async {
      await pump(tester, approvalOf('a1', tool: 'claude.Bash'), approver);
      expect(find.byKey(const ValueKey('approval-always-allow')), findsNothing);
      expect(find.text('Only this exact action, once. No "always allow".'), findsOneWidget);
    });

    testWidgets('no Always allow without the Approve right', (tester) async {
      await pump(tester, approvalOf('a1', tool: chromeTool, alwaysAllowEligible: true), reader);
      expect(find.byKey(const ValueKey('approval-always-allow')), findsNothing);
    });
  });

  group('inbox', () {
    late MockRepository repo;
    late StreamController<Event> changes;

    setUp(() {
      repo = MockRepository();
      changes = StreamController<Event>.broadcast();
      when(() => repo.online).thenAnswer((_) => const Stream.empty());
      when(() => repo.companyChanges).thenAnswer((_) => changes.stream);
      when(() => repo.linkState).thenReturn(approver);
      when(() => repo.notifications()).thenAnswer((_) async => const NotificationsSnapshot(notifications: [], unread: 0));
      when(() => repo.proposals()).thenAnswer((_) async => []);
    });

    tearDown(() => changes.close());

    blocTest<InboxBloc, InboxState>(
      'Always allow sends always_allow with the approval and says the tool no longer asks',
      build: () {
        when(() => repo.approvals()).thenAnswer((_) async => [approvalOf('a1', tool: chromeTool, alwaysAllowEligible: true)]);
        when(() => repo.write(any(), retry: false))
            .thenAnswer((_) async => receiptOf(const CommandResultApprovalDecided(approvalId: 'a1', status: ApprovalStatus.consumed, ok: true, output: '')));
        return InboxBloc(repo);
      },
      act: (bloc) async {
        await stateWhere(bloc, (s) => !s.loading);
        bloc.add(ApprovalDecided(bloc.state.approvals.single, approve: true, alwaysAllow: true));
        await stateWhere(bloc, (s) => s.notice?.startsWith('Approved.') ?? false);
      },
      verify: (_) {
        final request = verify(() => repo.write(captureAny(), retry: false)).captured.single as Map<String, dynamic>;
        expect(params(request)['always_allow'], isTrue);
        expect(params(request)['approve'], isTrue);
      },
      expect: () => contains(isA<InboxState>().having((s) => s.notice, 'notice', 'Approved. claude-in-chrome › navigate is always allowed from now on.')),
    );

    blocTest<InboxBloc, InboxState>(
      'an ask the server approved through Always allow leaves at once',
      build: () {
        when(() => repo.approvals()).thenAnswer((_) async => [approvalOf('a1', tool: chromeTool), approvalOf('a2')]);
        return InboxBloc(repo);
      },
      act: (bloc) async {
        await stateWhere(bloc, (s) => !s.loading);
        changes.add(eventOf(EventKind.approvalAllowedAlways, 'a1'));
        // The removal itself, not the debounced refresh after it (this mock would bring a1 back).
        await stateWhere(bloc, (s) => s.approvals.length == 1);
        expect(bloc.state.approvals.map((a) => a.id), ['a2']);
      },
    );
  });

  group('settings', () {
    late MockRepository repo;

    setUp(() {
      repo = MockRepository();
      when(() => repo.companyChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.linkState).thenReturn(approver);
    });

    Future<void> pump(WidgetTester tester) => tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
          value: repo,
          child: MaterialApp(theme: aureliusTheme(), home: const Scaffold(body: SingleChildScrollView(child: McpAlwaysAllowSection()))),
        ));

    testWidgets('lists always allowed tools, read-only', (tester) async {
      when(() => repo.mcpAlwaysAllowed()).thenAnswer((_) async => ['mcp__github__create_issue', 'mcp__claude-in-chrome__navigate']);
      await pump(tester);
      await tester.pump();
      expect(find.byKey(const ValueKey('mcp-always-allow-section')), findsOneWidget);
      expect(find.text('github › create_issue'), findsOneWidget);
      expect(find.text('claude-in-chrome › navigate'), findsOneWidget);
      expect(find.text('Remove'), findsNothing, reason: 'the list is changed on the desktop');
    });

    testWidgets('an empty list says how tools get there', (tester) async {
      when(() => repo.mcpAlwaysAllowed()).thenAnswer((_) async => []);
      await pump(tester);
      await tester.pump();
      expect(find.byKey(const ValueKey('mcp-always-allow-empty')), findsOneWidget);
    });

    testWidgets('servers without the list show nothing', (tester) async {
      when(() => repo.mcpAlwaysAllowed()).thenAnswer((_) async => throw TimeoutException('no answer'));
      await pump(tester);
      await tester.pump();
      expect(find.byKey(const ValueKey('mcp-always-allow-section')), findsNothing);
      expect(tester.takeException(), isNull);
    });
  });
}
