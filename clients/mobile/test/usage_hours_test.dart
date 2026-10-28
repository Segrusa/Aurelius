import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/usage/usage_page.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:intl/intl.dart';
import 'package:mocktail/mocktail.dart';

class MockRepository extends Mock implements AureliusRepository {}

final today = DateTime(2026, 10, 5);
final days = [for (var i = 2; i >= 0; i--) today.subtract(Duration(days: i)).millisecondsSinceEpoch];

/// The server's `usage_hours` value: 24 hours, busy at 09:00 on each day.
Map<String, dynamic> hoursJson(int dayMs, {int input = 1200, int? cost}) => {
      'day_start_ms': dayMs,
      'hours': [
        for (var h = 0; h < 24; h++)
          {
            'hour_start_ms': dayMs + h * 3600000,
            'runs': h == 9 ? 3 : 0,
            'input_tokens': h == 9 ? input : 0,
            'output_tokens': h == 9 ? 800 : 0,
            'cached_tokens': h == 9 ? 5000 : 0,
            'cost_micro_usd': h == 9 ? cost : null,
            'estimate': h == 9 && cost != null,
          },
      ],
    };

String eyebrow(int ms) => 'PER HOUR · ${DateFormat('EEE d MMM').format(DateTime.fromMillisecondsSinceEpoch(ms))}'.toUpperCase();

void main() {
  late MockRepository repo;

  void serve() {
    when(() => repo.linkState).thenReturn(const LinkState(status: LinkStatus.online));
    when(() => repo.usage()).thenAnswer((_) async => UsageSnapshot(byDay: [for (final d in days) UsageDay(dayStartMs: d, tokens: 2000)], bySource: const [], windows: const []));
    when(() => repo.subscription(refresh: any(named: 'refresh'))).thenAnswer((_) async => const SubscriptionUsage(fetchedAtMs: 0, limits: [], raw: ''));
    when(() => repo.usageHours(days[0])).thenAnswer((_) async => UsageHours.fromJson(hoursJson(days[0], input: 100)));
    when(() => repo.usageHours(days[1])).thenAnswer((_) async => UsageHours.fromJson(hoursJson(days[1], input: 200)));
    when(() => repo.usageHours(days[2])).thenAnswer((_) async => UsageHours.fromJson(hoursJson(days[2], cost: 42000)));
  }

  Future<void> pumpPage(WidgetTester tester) async {
    tester.view.physicalSize = const Size(1170, 2532);
    tester.view.devicePixelRatio = 3;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
      value: repo,
      child: MaterialApp(theme: aureliusTheme(), home: const UsagePage()),
    ));
    await tester.pumpAndSettle();
  }

  setUp(() => repo = MockRepository());

  test('usage_hours reads the server value, with a cost only when one hour has it', () {
    final hours = UsageHours.fromJson(hoursJson(days[2], cost: 42000));
    expect(hours.hours, hasLength(24));
    final busy = hours.hours[9];
    expect((busy.hourStartMs, busy.runs, busy.inputTokens, busy.outputTokens, busy.cachedTokens, busy.costMicroUsd, busy.estimate), (days[2] + 9 * 3600000, 3, 1200, 800, 5000, 42000, true));
    expect((hours.runs, hours.inputTokens, hours.outputTokens, hours.costMicroUsd), (3, 1200, 800, 42000));
    expect(UsageHours.fromJson(hoursJson(days[2])).costMicroUsd, isNull);
  });

  testWidgets('today is shown by default; a tapped day, prev and next switch the hours', (tester) async {
    serve();
    await pumpPage(tester);
    verify(() => repo.usageHours(days[2])).called(1);
    expect(find.text(eyebrow(days[2])), findsOneWidget);
    expect(find.text('2.0k'), findsWidgets, reason: 'day total');
    expect(find.byKey(const ValueKey('usage-next-day')), findsOneWidget);
    expect(tester.widget<IconButton>(find.byKey(const ValueKey('usage-next-day'))).onPressed, isNull, reason: 'today is the last day');

    // A bar shows its hour.
    await tester.ensureVisible(find.byKey(const ValueKey('usage-hour-9')));
    await tester.tap(find.byKey(const ValueKey('usage-hour-9')));
    await tester.pump();
    expect(find.text('09:00–10:00'), findsOneWidget);
    expect(find.text('5.0k'), findsOneWidget, reason: 'cached');
    expect(find.text('≈ \$0.04'), findsWidgets, reason: 'cost, known for this day');

    await tester.tap(find.byKey(ValueKey('usage-day-${days[0]}')));
    await tester.pumpAndSettle();
    expect(find.text(eyebrow(days[0])), findsOneWidget);
    expect(find.byKey(const ValueKey('usage-hour-detail')), findsNothing, reason: 'another day starts unpicked');
    expect(tester.widget<IconButton>(find.byKey(const ValueKey('usage-prev-day'))).onPressed, isNull, reason: 'the first of the 14 days');

    await tester.tap(find.byKey(const ValueKey('usage-next-day')));
    await tester.pumpAndSettle();
    expect(find.text(eyebrow(days[1])), findsOneWidget);
    verify(() => repo.usageHours(days[1])).called(1);
    await tester.tap(find.byKey(const ValueKey('usage-prev-day')));
    await tester.pumpAndSettle();
    expect(find.text(eyebrow(days[0])), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
