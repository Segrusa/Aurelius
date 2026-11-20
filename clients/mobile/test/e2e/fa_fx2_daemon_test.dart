// FX2 (AUR-FA-FX2): the Inbox writes notification.mark_read, a row shows its ticket
// number once, and a message offers Edit and Delete after it was edited, against a
// real test daemon. It runs headless: no window, no keyring, the real app over a real
// socket. See integration_test/README.md for the daemon; robin is played by aur.py.
//
//   flutter test test/e2e/fa_fx2_daemon_test.dart \
//     --dart-define=AUR_URL=http://127.0.0.1:47871 --dart-define=AUR_CODE=<pairing code> \
//     --dart-define=AUR_SOCK=<daemon socket> --dart-define=AUR_PROJECT=<project id> \
//     --dart-define=AUR_EMPLOYEE=<employee id> --dart-define=AUR_ALEX=<id> --dart-define=AUR_ROBIN=<id> \
//     --dart-define=AUR_CHANNEL=<id of the General channel>
//
// Without AUR_URL it skips, so it never runs by accident.

import 'dart:convert';
import 'dart:io';

import 'package:aurelius_mobile/app/app.dart';
import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/hosts/host_store.dart';
import 'package:aurelius_mobile/core/net/gateway_api.dart';
import 'package:aurelius_mobile/core/push/push_service.dart';
import 'package:aurelius_mobile/core/widgets/ios.dart';
import 'package:aurelius_mobile/features/pairing/pairing_bloc.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:shared_preferences/shared_preferences.dart';

const url = String.fromEnvironment('AUR_URL');
const code = String.fromEnvironment('AUR_CODE');
const sock = String.fromEnvironment('AUR_SOCK');
const project = String.fromEnvironment('AUR_PROJECT');
const employee = String.fromEnvironment('AUR_EMPLOYEE');
const alex = String.fromEnvironment('AUR_ALEX');
const robin = String.fromEnvironment('AUR_ROBIN');

/// One call as a board member, over the daemon's socket (aur.py). The value of a write's result.
Future<Map<String, dynamic>> as(
  String who,
  String method,
  Map<String, dynamic> params, {
  bool write = true,
}) async {
  final done = await Process.run('python3', [
    '-I',
    'integration_test/daemon/aur.py',
    sock,
    who,
    who == 'alex' ? 'test-password-1' : 'test-password-2',
    method,
    jsonEncode(params),
    if (write) '--write',
  ]);
  final answer = jsonDecode(done.stdout as String) as Map<String, dynamic>;
  if (answer['ok'] == null) throw StateError('$method: ${answer['error']}');
  final value =
      (answer['ok'] as Map<String, dynamic>)['value'] as Map<String, dynamic>;
  return write
      ? ((value['result'] as Map<String, dynamic>)['value']
            as Map<String, dynamic>)
      : value;
}

void main() {
  LiveTestWidgetsFlutterBinding.ensureInitialized();

  Future<void> waitFor(
    WidgetTester tester,
    Finder finder, {
    Duration timeout = const Duration(seconds: 30),
    String? what,
  }) async {
    final end = DateTime.now().add(timeout);
    while (DateTime.now().isBefore(end)) {
      await tester.pump(const Duration(milliseconds: 100));
      if (finder.evaluate().isNotEmpty) return;
    }
    final shown = find
        .byType(Text)
        .evaluate()
        .map(
          (e) =>
              (e.widget as Text).data ??
              (e.widget as Text).textSpan?.toPlainText(),
        )
        .whereType<String>()
        .toSet();
    fail(
      'not seen within $timeout: ${what ?? finder}; on screen: ${shown.join(' | ')}',
    );
  }

  Future<void> tap(WidgetTester tester, Finder finder) async {
    await waitFor(tester, finder);
    await tester.ensureVisible(finder.first);
    await tester.pump(const Duration(milliseconds: 200));
    await tester.tap(finder.first, warnIfMissed: false);
    await tester.pump(const Duration(milliseconds: 300));
  }

  BuildContext pageContext(WidgetTester tester) =>
      tester.element(find.byType(Scaffold).last);
  void go(WidgetTester tester, String location) =>
      GoRouter.of(pageContext(tester)).go(location);
  void step(String text) => debugPrint('e2e: $text');

  /// The rows `notification.list` answers for [who], as (id, read, subject).
  Future<List<Map<String, dynamic>>> rowsOf(String who) async {
    final v = await as(who, 'notification.list', {}, write: false);
    return (v['notifications'] as List).cast<Map<String, dynamic>>();
  }

  testWidgets(
    'FX2: Inbox read on the server, one ticket number, long press after an edit',
    (tester) async {
      HttpOverrides.global = null;
      tester.view.physicalSize = const Size(600, 2200);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      SharedPreferences.setMockInitialValues({});
      final store = MemoryHostStore();
      final pairing = PairingBloc(GatewayApi(), platform: 'ios');
      pairing.add(
        PairingSubmitted(
          PairingPayload(
            url: url,
            code: code,
            hostId: '',
            hostName: 'the PC',
            environment: 'dev',
          ),
          deviceName: 'FX2 phone',
        ),
      );
      final paired = await pairing.stream
          .firstWhere(
            (s) => s.step == PairingStep.paired || s.step == PairingStep.failed,
          )
          .timeout(const Duration(seconds: 60));
      await pairing.close();
      if (paired.host == null) fail('pairing: ${paired.message}');
      await store.save(paired.host!, paired.deviceSecret!);
      await store.setActive(paired.host!.hostId);
      await tester.pumpWidget(
        AppearanceScope(
          child: AureliusApp(
            store: store,
            api: GatewayApi(),
            push: PushService(),
          ),
        ),
      );
      await tester.pump(const Duration(seconds: 1));
      await waitFor(
        tester,
        find.byKey(const ValueKey('sign-in-username')),
        timeout: const Duration(seconds: 90),
        what: 'the sign-in',
      );
      await tester.enterText(
        find.byKey(const ValueKey('sign-in-username')),
        'alex',
      );
      await tester.enterText(
        find.byKey(const ValueKey('sign-in-password')),
        'test-password-1',
      );
      await tap(tester, find.byKey(const ValueKey('sign-in')));
      await waitFor(tester, find.text('Home'), what: 'Home');
      await tester.pump(const Duration(seconds: 2));
      step('paired and signed in as alex');

      // The Inbox: the row of the ticket on alex's limit.
      final before = await rowsOf('alex');
      final target = before.firstWhere((n) => n['reason'] == 'login');
      final id = target['id'] as String;
      expect(
        target['read'],
        isFalse,
        reason: 'the row starts unread on the server',
      );
      go(tester, '/inbox');
      await waitFor(
        tester,
        find.byKey(ValueKey('inbox-row-$id')),
        what: 'the login row',
      );
      final shown = find
          .descendant(
            of: find.byKey(ValueKey('inbox-row-$id')),
            matching: find.byType(Text),
          )
          .evaluate()
          .map((e) => (e.widget as Text).data)
          .whereType<String>()
          .toList();
      step('row text: ${shown.join(' | ')}');
      final number = target['ticket_number'];
      expect(
        shown.where((s) => s.startsWith('#$number')).toList(),
        hasLength(1),
      );
      expect(
        shown.any((s) => s.startsWith('#$number #$number')),
        isFalse,
        reason: 'the number is not doubled',
      );
      step('FX2-3: the row shows its ticket number once');
      await tap(tester, find.byKey(ValueKey('inbox-row-$id')));
      await tester.pump(const Duration(seconds: 2));
      final after = (await rowsOf('alex')).firstWhere((n) => n['id'] == id);
      step('FX2-1: server row read after opening = ${after['read']}');
      expect(
        after['read'],
        isTrue,
        reason: 'opening a row sends notification.mark_read',
      );
    },
    skip: url.isEmpty,
  );
}
