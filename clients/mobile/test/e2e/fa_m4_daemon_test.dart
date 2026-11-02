// A5b: links the operating system opens (cold and warm start) against a real test
// daemon with F3 (AUR-FA-M4). The system is a stream the test feeds. It runs headless: no window,
// no keyring, the real app over a real socket. See integration_test/README.md for
// the daemon; robin is played by aur.py.
//
//   flutter test test/e2e/fa_m2_daemon_test.dart \
//     --dart-define=AUR_URL=http://127.0.0.1:47871 --dart-define=AUR_CODE=<pairing code> \
//     --dart-define=AUR_SOCK=<daemon socket> --dart-define=AUR_PROJECT=<project id> \
//     --dart-define=AUR_EMPLOYEE=<employee id> --dart-define=AUR_ALEX=<id> --dart-define=AUR_ROBIN=<id> \
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
import 'package:aurelius_mobile/core/links/os_links.dart';
import 'package:aurelius_mobile/data/links.dart';
import 'package:aurelius_mobile/features/links/link_actions.dart';
import 'package:aurelius_mobile/features/pairing/pairing_bloc.dart';

import 'dart:async' show StreamController;

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

  testWidgets('A5b against the F3 daemon', (tester) async {
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
        deviceName: 'M4 phone',
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
    // Cold start: the system's link is there before the app is up and signed in.
    final system = StreamController<Uri>();
    final listening = listenForOsLinks(system.stream);
    addTearDown(() async {
      await listening.cancel();
      await system.close();
      pendingAureliusLink = null;
    });
    final made = await as('alex', 'ticket.create', {
      'project_id': project,
      'title': 'Opened by the system',
    });
    final ticket = made['ticket_id'] as String;
    final hostId = paired.host!.hostId;
    system.add(Uri.parse('aurelius://$hostId/ticket/$ticket'));
    await tester.pump(const Duration(milliseconds: 100));
    expect(
      pendingAureliusLink,
      'aurelius://$hostId/ticket/$ticket',
      reason: 'held in memory, not opened yet',
    );
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
    step('cold start: link held until the server is connected and signed in');
    await waitFor(
      tester,
      find.text('Opened by the system'),
      timeout: const Duration(seconds: 60),
      what: 'the ticket of the cold-start link',
    );
    expect(pendingAureliusLink, isNull);
    step('cold start: after sign-in the ticket opened by itself');
    go(tester, '/home');
    await tester.pump(const Duration(seconds: 1));

    // Warm start: the app is up and online.
    final sent = await as('alex', 'group_chat.send', {
      'text': 'A message to open',
    });
    final message = sent['message_id'] as String;
    system.add(Uri.parse('aurelius://$hostId/message/$message'));
    await waitFor(
      tester,
      find.byKey(ValueKey('flash-$message')),
      timeout: const Duration(seconds: 60),
      what: 'the message of the warm-start link, marked',
    );
    step('warm start: the message opened at its place and is marked');
    go(tester, '/home');
    await tester.pump(const Duration(seconds: 1));

    system.add(Uri.parse('aurelius://$hostId/ticket/$ticket?token=secret'));
    await waitFor(
      tester,
      find.text(invalidLinkText),
      what: 'the fixed text for a link with a query',
    );
    step('warm start: a link with a query says "$invalidLinkText"');
    await tester.pump(const Duration(seconds: 4));
    system.add(
      Uri.parse(
        'aurelius://01a19c46-0000-7000-8000-000000000000/ticket/$ticket',
      ),
    );
    await waitFor(
      tester,
      find.text("This link is for a server this device isn't paired with."),
      what: 'a link of another server',
    );
    step(
      'warm start: a link for a server this phone is not paired with is not opened',
    );
  }, skip: url.isEmpty);
}
