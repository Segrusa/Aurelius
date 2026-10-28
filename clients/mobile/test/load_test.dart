import 'dart:async';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/core/widgets/widgets.dart' show InfoIcon;
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/chat/chat_bloc.dart';
import 'package:aurelius_mobile/features/load/load_widgets.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';

class MockRepository extends Mock implements AureliusRepository {}

/// The load answer as the server writes it, with [changes] on top.
Map<String, dynamic> loadJson([Map<String, dynamic> changes = const {}]) => {
      'checks_active': true,
      'cpu_pct': 82.4,
      'gate': 'held',
      'held': 1,
      'hold': {'kind': 'cpu', 'cpu_pct': 82.4},
      'limits': {'max_runs': 3, 'cpu_high': 75, 'cpu_low': 65, 'mem_min_mb': 2048, 'enabled': true},
      'mem_free_mb': 9216,
      'mem_total_mb': 32768,
      'running': 2,
      'sampled_at_ms': 1,
      ...changes,
    };

LoadSnapshot snapshot([Map<String, dynamic> changes = const {}]) => LoadSnapshot.fromJson(loadJson(changes));

const limits = LoadLimits(maxRuns: 3, cpuHigh: 75, cpuLow: 65, memMinMb: 2048, enabled: true);

MessageView message(String id, MessageRole role, {MessageStatus status = MessageStatus.complete, String? update}) => messageOf(id, role, id, status: status, update: update);

void main() {

  group('wire', () {
    test('load/get reads the snapshot', () {
      final s = snapshot();
      expect(s.cpuPct, closeTo(82.4, 0.01));
      expect((s.memFreeMb, s.memTotalMb, s.running, s.held, s.gateHeld, s.checksActive), (9216, 32768, 2, 1, true, true));
      expect(s.hold!.kind, LoadHoldKind.cpu);
      expect(s.hold!.cpuPct, 82.4);
      expect(s.limits.toJson(), {'cpu_high': 75, 'cpu_low': 65, 'enabled': true, 'max_runs': 3, 'mem_min_mb': 2048});
    });

    test('load_changed is its own server message', () {
      final message = ServerMessage.fromJson({
        'type': 'load_changed',
        ...loadJson({'hold': {'kind': 'memory', 'mem_free_mb': 1434}, 'mem_free_mb': 1434, 'checks_active': false}),
      });
      expect(message, isA<ServerMessageLoadChanged>());
      final s = (message as ServerMessageLoadChanged).value;
      expect(s.hold?.kind, LoadHoldKind.memory);
      expect(s.checksActive, isFalse);
      expect(s.waitingText, 'Server busy: 1.4 GB free, waiting');
    });

    test('the banner text per hold, with live values', () {
      expect(snapshot().waitingText, 'Server busy: CPU 82%, waiting');
      expect(snapshot({'hold': {'kind': 'max_runs', 'running': 3}, 'running': 3}).waitingText, 'Server busy: 3 runs active, waiting');
      expect(snapshot({'hold': {'kind': 'max_runs'}, 'running': 1}).waitingText, 'Server busy: 1 run active, waiting');
      expect(snapshot({'hold': {'kind': 'disabled_all'}}).waitingText, 'Runs paused on the server');
      expect(snapshot({'hold': null, 'gate': 'open'}).waitingText, isNull);
    });

    test('limits are checked here first', () {
      expect(limits.problem, isNull);
      expect(const LoadLimits(maxRuns: 3, cpuHigh: 70, cpuLow: 70, memMinMb: 0, enabled: true).problem, 'CPU low must be below CPU high.');
      expect(const LoadLimits(maxRuns: 3, cpuHigh: 101, cpuLow: 50, memMinMb: 0, enabled: true).problem, 'CPU high can be at most 100%.');
    });
  });

  group('a conversation awaits an answer that has not started', () {
    test('only after its own message, while nothing streams', () {
      expect(const ChatState().awaitingStart, isFalse);
      expect(ChatState(messages: [message('u', MessageRole.user)]).awaitingStart, isTrue);
      expect(ChatState(messages: [message('u', MessageRole.user), message('a', MessageRole.assistant)]).awaitingStart, isFalse);
      expect(ChatState(messages: [message('u', MessageRole.user), message('a', MessageRole.assistant, status: MessageStatus.partial)]).awaitingStart, isFalse);
      expect(ChatState(messages: [message('a', MessageRole.assistant), message('b', MessageRole.assistant, update: 'brief')]).awaitingStart, isTrue);
      expect(ChatState(messages: [message('a', MessageRole.assistant), message('m', MessageRole.assistant, update: 'message')]).awaitingStart, isTrue);
      expect(ChatState(messages: [message('a', MessageRole.assistant), message('p', MessageRole.assistant, update: 'plan')]).awaitingStart, isFalse);
      expect(const ChatState(unsent: [UnsentMessage(localId: 'l', idempotencyKey: 'c', text: 'hi', delivered: true)]).awaitingStart, isTrue);
    });
  });

  group('screens', () {
    late MockRepository repo;
    late StreamController<LoadSnapshot> changes;

    void serve({List<String> capabilities = const ['read', 'chat', 'approve'], LoadSnapshot? first}) {
      when(() => repo.linkState).thenReturn(LinkState(status: LinkStatus.online, capabilities: capabilities));
      when(() => repo.loadChanges).thenAnswer((_) => changes.stream);
      when(() => repo.online).thenAnswer((_) => const Stream.empty());
      when(() => repo.load()).thenAnswer((_) async => first ?? snapshot());
    }

    Future<void> pump(WidgetTester tester, Widget child) async {
      tester.view.physicalSize = const Size(1170, 2532);
      tester.view.devicePixelRatio = 3;
      addTearDown(tester.view.reset);
      await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
        value: repo,
        child: MaterialApp(theme: aureliusTheme(), home: Scaffold(body: ListView(children: [child]))),
      ));
      await tester.pumpAndSettle();
    }

    setUp(() {
      repo = MockRepository();
      changes = StreamController<LoadSnapshot>.broadcast();
    });
    tearDown(() => changes.close());

    testWidgets('the banner follows load_changed and goes once runs may start', (tester) async {
      serve();
      await pump(tester, const LoadBanner(waiting: true));
      expect(find.text('Server busy: CPU 82%, waiting'), findsOneWidget);

      changes.add(snapshot({'cpu_pct': 91.2}));
      await tester.pumpAndSettle();
      expect(find.text('Server busy: CPU 91%, waiting'), findsOneWidget);

      changes.add(snapshot({'hold': {'kind': 'max_runs', 'running': 3}, 'running': 3}));
      await tester.pumpAndSettle();
      expect(find.text('Server busy: 3 runs active, waiting'), findsOneWidget);

      changes.add(snapshot({'hold': null, 'gate': 'open', 'held': 0}));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('load-banner')), findsNothing);
    });

    testWidgets('no banner once the answer started', (tester) async {
      serve();
      await pump(tester, const LoadBanner(waiting: false));
      expect(find.byKey(const ValueKey('load-banner')), findsNothing);
    });

    testWidgets('settings show the live meter and the limits, read-only', (tester) async {
      serve(first: snapshot({'checks_active': false}));
      await pump(tester, LoadLimitsSection(link: repo.linkState));

      expect(find.text('82%'), findsOneWidget);
      expect(find.text('9.0 GB of 32 GB'), findsOneWidget);
      expect(find.text('2 running · 1 waiting'), findsOneWidget);
      expect(find.text('Checks only active on the standard data folder'), findsOneWidget);
      expect(find.text('75%'), findsOneWidget);
      expect(find.text('2048 MB'), findsOneWidget);
      expect(find.byType(InfoIcon), findsOneWidget);
      expect(find.byType(TextField), findsNothing, reason: 'limits are changed on the desktop');

      // Live.
      changes.add(snapshot({'cpu_pct': 40.0, 'hold': null, 'gate': 'open', 'held': 0, 'running': 1, 'limits': {'max_runs': 3, 'cpu_high': 90, 'cpu_low': 65, 'mem_min_mb': 2048, 'enabled': false}}));
      await tester.pumpAndSettle();
      expect(find.text('40%'), findsOneWidget);
      expect(find.text('90%'), findsOneWidget);
      expect(find.text('Off'), findsOneWidget);
      expect(find.text('1 running · 0 waiting'), findsOneWidget);
      expect(find.text('Checks only active on the standard data folder'), findsNothing);
    });
  });
}
