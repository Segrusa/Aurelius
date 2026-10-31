import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/protocol/models.dart';
import 'package:aurelius_mobile/features/more/app_keys_section.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

class MockRepository extends Mock implements AureliusRepository {}

AppKeyView keyOf(String id, {AppAudience? audience, String? webhookUrl, List<AppScope> scopes = const [AppScope.read]}) => AppKeyView(
      audience: audience,
      createdAtMs: 0,
      id: id,
      name: 'app-$id',
      scopes: scopes,
      tasksLastHour: 0,
      tasksOpen: 0,
      webhookUrl: webhookUrl,
    );

void main() {
  setUpAll(() => registerFallbackValue(const CreateAppKey(name: 'x', scopes: [])));

  test('webhookUrlProblem accepts empty, http and https only', () {
    expect(webhookUrlProblem(''), isNull);
    expect(webhookUrlProblem('https://hooks.example/a'), isNull);
    expect(webhookUrlProblem('http://localhost:8080/x'), isNull);
    expect(webhookUrlProblem('ftp://example.com'), isNotNull);
    expect(webhookUrlProblem('example.com'), isNotNull);
    expect(webhookUrlProblem('https://'), isNotNull);
  });

  group('AppKeysSection', () {
    late MockRepository repo;

    LinkState link(List<String> features) => LinkState(status: LinkStatus.online, capabilities: const ['approve'], features: features);

    Future<void> pump(WidgetTester tester, List<String> features, List<AppKeyView> keys) async {
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (call) async => null);
      tester.view.physicalSize = const Size(800, 2400);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      repo = MockRepository();
      when(() => repo.linkState).thenReturn(link(features));
      when(() => repo.appKeys()).thenAnswer((_) async => keys);
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(
          body: RepositoryProvider<AureliusRepository>.value(value: repo, child: const SingleChildScrollView(child: AppKeysSection())),
        ),
      ));
      await tester.pumpAndSettle();
    }

    testWidgets('lists audience and webhook address, never a secret', (tester) async {
      await pump(tester, ['app_keys', 'management_channel', 'webhooks'], [
        keyOf('1', audience: AppAudience.management, webhookUrl: 'https://hooks.example/m'),
        keyOf('2', audience: AppAudience.all),
      ]);
      expect(find.textContaining('Audience: management'), findsOneWidget);
      expect(find.textContaining('Audience: all'), findsOneWidget);
      expect(find.textContaining('https://hooks.example/m'), findsOneWidget);
      expect(find.textContaining('whsec_'), findsNothing);
    });

    testWidgets('hidden without the app_keys feature', (tester) async {
      await pump(tester, [], []);
      expect(find.byKey(const ValueKey('app-keys-section')), findsNothing);
    });

    testWidgets('audience and webhook fields follow the server features', (tester) async {
      await pump(tester, ['app_keys'], []);
      await tester.tap(find.byKey(const ValueKey('app-key-create')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('app-key-audience')), findsNothing);
      expect(find.byKey(const ValueKey('app-key-webhook')), findsNothing);
    });

    testWidgets('B1-2: a new key says on whose Claude login its calls run', (tester) async {
      await pump(tester, ['app_keys'], []);
      await tester.tap(find.byKey(const ValueKey('app-key-create')));
      await tester.pumpAndSettle();
      expect(find.text('Tasks of this app run on your own Claude subscription.'), findsOneWidget);
      expect(find.text('Its calls run on the Claude login of the board member who made it or last set its functions. Only for your own use: don\'t give this key to others. For calls from others, use an employee on an Anthropic API key connection.'), findsOneWidget);
    });

    testWidgets('management turns approve off, bad address is refused, secrets are shown once', (tester) async {
      await pump(tester, ['app_keys', 'management_channel', 'webhooks'], []);
      when(() => repo.createAppKey(any())).thenAnswer((_) async => AppKeyCreated(
            key: keyOf('9', audience: AppAudience.management, webhookUrl: 'https://hooks.example/m'),
            secret: 'amk_secret',
            webhookSecret: 'whsec_secret',
          ));
      await tester.tap(find.byKey(const ValueKey('app-key-create')));
      await tester.pumpAndSettle();
      await tester.enterText(find.byKey(const ValueKey('app-key-name')), 'Marcus');
      await tester.tap(find.byKey(const ValueKey('app-key-scope-approve')));
      await tester.pump();
      await tester.tap(find.text('Management'));
      await tester.pump();
      expect(tester.widget<FilterChip>(find.byKey(const ValueKey('app-key-scope-approve'))).selected, isFalse);
      expect(tester.widget<FilterChip>(find.byKey(const ValueKey('app-key-scope-approve'))).onSelected, isNull);

      await tester.enterText(find.byKey(const ValueKey('app-key-webhook')), 'ftp://nope');
      await tester.tap(find.byKey(const ValueKey('app-key-submit')));
      await tester.pump();
      expect(find.byKey(const ValueKey('app-key-problem')), findsOneWidget);
      verifyNever(() => repo.createAppKey(any()));

      await tester.enterText(find.byKey(const ValueKey('app-key-webhook')), 'https://hooks.example/m');
      await tester.tap(find.byKey(const ValueKey('app-key-submit')));
      await tester.pumpAndSettle();
      final sent = verify(() => repo.createAppKey(captureAny())).captured.single as CreateAppKey;
      expect(sent.audience, AppAudience.management);
      expect(sent.webhookUrl, 'https://hooks.example/m');
      expect(sent.scopes.contains(AppScope.approve), isFalse);

      expect(find.text('amk_secret'), findsOneWidget);
      expect(find.text('whsec_secret'), findsOneWidget);
      await tester.tap(find.text('Done'));
      await tester.pumpAndSettle();
      expect(find.text('whsec_secret'), findsNothing);
    });
  });
}
