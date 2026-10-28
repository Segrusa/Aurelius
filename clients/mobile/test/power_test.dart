import 'dart:async';

import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/power/power_bloc.dart';
import 'package:aurelius_mobile/features/power/power_widgets.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

class MockRepository extends Mock implements AureliusRepository {}

PowerState idle() => const PowerState();
PowerState at(int deadline) => PowerState(pending: PendingShutdown(when: ShutdownWhen.now, requestedAtMs: 0, deadlineMs: deadline));
const waiting = PowerState(pending: PendingShutdown(when: ShutdownWhen.whenIdle, requestedAtMs: 0));

void main() {
  group('wire', () {
    test('power.get and power_changed carry the pending shutdown', () {
      expect(PowerState.fromJson({'pending': null}).pending, isNull);
      final p = PowerState.fromJson({
        'pending': {'when': 'now', 'requested_at_ms': 5, 'deadline_ms': 60000},
      }).pending!;
      expect((p.when, p.requestedAtMs, p.deadlineMs), (ShutdownWhen.now, 5, 60000));
      expect(
        PowerState.fromJson({
          'pending': {'when': 'when_idle', 'requested_at_ms': 5, 'deadline_ms': null},
        }).pending!.deadlineMs,
        isNull,
      );

      final message = ServerMessage.fromJson({'type': 'power_changed', 'pending': null});
      expect(message, isA<ServerMessagePowerChanged>());
      expect((message as ServerMessagePowerChanged).value.pending, isNull);
      expect(ShutdownWhen.now.wire, 'now');
      expect(ShutdownWhen.whenIdle.wire, 'when_idle');
    });

    test('countdown text', () {
      expect(countdownText(const Duration(seconds: 45)), '0:45');
      expect(countdownText(const Duration(milliseconds: 44100)), '0:45');
      expect(countdownText(const Duration(minutes: 12, seconds: 3)), '12:03');
      expect(countdownText(const Duration(hours: 1, minutes: 2, seconds: 3)), '1:02:03');
      expect(countdownText(const Duration(seconds: -4)), '0:00');
      expect(pendingText(waiting.pending!, 0), 'PC shuts down when all work is done');
      expect(pendingText(at(45000).pending!, 0), 'PC shuts down in 0:45');
    });
  });

  group('screens', () {
    late MockRepository repo;
    late StreamController<PowerState> changes;
    var now = 0;

    void serve({List<String> capabilities = const ['read', 'chat', 'owner'], PowerState first = const PowerState()}) {
      when(() => repo.linkState).thenReturn(LinkState(status: LinkStatus.online, capabilities: capabilities));
      when(() => repo.powerChanges).thenAnswer((_) => changes.stream);
      when(() => repo.online).thenAnswer((_) => const Stream.empty());
      when(() => repo.power()).thenAnswer((_) async => first);
    }

    Future<void> pump(WidgetTester tester) async {
      tester.view.physicalSize = const Size(1170, 2532);
      tester.view.devicePixelRatio = 3;
      addTearDown(tester.view.reset);
      await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
        value: repo,
        child: BlocProvider(
          create: (_) => PowerBloc(repo),
          child: MaterialApp(
            theme: aureliusTheme(),
            home: Scaffold(
              body: Column(children: [PowerBanner(now: () => now), PowerSection(link: repo.linkState, serverName: 'Desk')]),
            ),
          ),
        ),
      ));
      await tester.pumpAndSettle();
    }

    setUpAll(() => registerFallbackValue(ShutdownWhen.now));
    setUp(() {
      repo = MockRepository();
      changes = StreamController<PowerState>.broadcast();
      now = 0;
    });
    tearDown(() => changes.close());

    testWidgets('the banner counts down live and follows power_changed', (tester) async {
      serve(first: at(45000));
      await pump(tester);
      expect(find.text('PC shuts down in 0:45'), findsOneWidget);

      now = 3000;
      await tester.pump(const Duration(seconds: 3));
      expect(find.text('PC shuts down in 0:42'), findsOneWidget);

      changes.add(waiting);
      await tester.pumpAndSettle();
      expect(find.text('PC shuts down when all work is done'), findsOneWidget);

      changes.add(idle());
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('power-banner')), findsNothing);
    });

    testWidgets('Cancel withdraws the shutdown', (tester) async {
      serve(first: waiting);
      when(() => repo.cancelShutdown()).thenAnswer((_) async => idle());
      await pump(tester);

      await tester.tap(find.byKey(const ValueKey('power-cancel')));
      await tester.pumpAndSettle();
      verify(() => repo.cancelShutdown()).called(1);
      expect(find.byKey(const ValueKey('power-banner')), findsNothing);
    });

    testWidgets('without the Owner right there is no banner and nothing is asked', (tester) async {
      serve(capabilities: const ['read', 'chat', 'approve'], first: waiting);
      await pump(tester);
      expect(find.byKey(const ValueKey('power-banner')), findsNothing);
      verifyNever(() => repo.power());
    });

    testWidgets('the settings rows ask first, warn about lost answers, then shut down', (tester) async {
      serve();
      when(() => repo.shutdownPc(any())).thenAnswer((_) async => waiting);
      await pump(tester);

      await tester.tap(find.byKey(const ValueKey('power-now')));
      await tester.pumpAndSettle();
      expect(find.textContaining('Running answers are lost'), findsOneWidget);
      verifyNever(() => repo.shutdownPc(any()));
      await tester.tap(find.byKey(const ValueKey('glass-confirm-action')));
      await tester.pumpAndSettle();
      verify(() => repo.shutdownPc(ShutdownWhen.now)).called(1);

      await tester.tap(find.byKey(const ValueKey('power-idle')));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('glass-confirm-action')));
      await tester.pumpAndSettle();
      verify(() => repo.shutdownPc(ShutdownWhen.whenIdle)).called(1);
    });

    testWidgets('Cancel in the dialog shuts nothing down', (tester) async {
      serve();
      await pump(tester);
      await tester.tap(find.byKey(const ValueKey('power-now')));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      verifyNever(() => repo.shutdownPc(any()));
    });

    testWidgets('without the Owner right the settings rows are hidden', (tester) async {
      serve(capabilities: const ['read', 'chat', 'approve']);
      await pump(tester);
      expect(find.byKey(const ValueKey('power-now')), findsNothing);
      expect(find.byKey(const ValueKey('power-idle')), findsNothing);
    });
  });
}
