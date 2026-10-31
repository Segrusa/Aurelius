// The smoke run of AUR-MU-03 (Phase 3) against a real test daemon, as the
// Linux desktop build: pair, sign in, the session list, Connect with the
// stand-in claude, a ticket that waits for another board member's login and
// Run on my login. See integration_test/README.md for the daemon it needs.
//
//   flutter test integration_test/smoke_test_daemon_test.dart -d linux \
//     --dart-define=AUR_URL=http://127.0.0.1:47792 --dart-define=AUR_CODE=<pairing code> \
//     --dart-define=AUR_USER=alex --dart-define=AUR_PASSWORD=<password> \
//     --dart-define=AUR_TICKET=<ticket id> [--dart-define=AUR_LIMIT_TICKET=<ticket id>] [--dart-define=AUR_RESET=true]
//
// Without AUR_URL it skips, so it never runs by accident.

import 'package:aurelius_mobile/app/app.dart';
import 'package:aurelius_mobile/core/hosts/host_store.dart';
import 'package:aurelius_mobile/core/net/gateway_api.dart';
import 'package:aurelius_mobile/core/push/push_service.dart';
import 'package:aurelius_mobile/core/widgets/ios.dart';
import 'package:flutter/material.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:integration_test/integration_test.dart';
import 'package:shared_preferences/shared_preferences.dart';

const url = String.fromEnvironment('AUR_URL');
const code = String.fromEnvironment('AUR_CODE');
const user = String.fromEnvironment('AUR_USER', defaultValue: 'alex');
const password = String.fromEnvironment('AUR_PASSWORD');
const ticket = String.fromEnvironment('AUR_TICKET');
const limitTicket = String.fromEnvironment('AUR_LIMIT_TICKET');
/// Forget this app's own pairings and sessions first (its secure storage and preferences).
const reset = bool.fromEnvironment('AUR_RESET');

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  /// Pumps real time until [finder] shows (the daemon answers over a socket).
  Future<void> waitFor(WidgetTester tester, Finder finder, {Duration timeout = const Duration(seconds: 30), String? what}) async {
    final end = DateTime.now().add(timeout);
    while (DateTime.now().isBefore(end)) {
      await tester.pump(const Duration(milliseconds: 200));
      if (finder.evaluate().isNotEmpty) return;
    }
    final shown = find.byType(Text).evaluate().map((e) => (e.widget as Text).data ?? (e.widget as Text).textSpan?.toPlainText()).whereType<String>().toSet();
    fail('not seen within $timeout: ${what ?? finder}; on screen: ${shown.join(' | ')}');
  }

  Future<void> tap(WidgetTester tester, Finder finder) async {
    await tester.ensureVisible(finder.first);
    await tester.pump(const Duration(milliseconds: 200));
    await tester.tap(finder.first);
    await tester.pump(const Duration(milliseconds: 300));
  }

  void go(WidgetTester tester, String location) => GoRouter.of(tester.element(find.byType(Navigator).first)).go(location);

  testWidgets('Phase 3 smoke run against a test daemon', (tester) async {
    if (reset) {
      await const FlutterSecureStorage().deleteAll();
      await (await SharedPreferences.getInstance()).clear();
      debugPrint('smoke: forgot the earlier pairings and sessions of this app');
    }
    await tester.pumpWidget(AppearanceScope(child: AureliusApp(store: DeviceHostStore(), api: GatewayApi(), push: PushService())));
    await tester.pump(const Duration(seconds: 1));

    // 1. Pair: the manual address and code (the test script allows it on the daemon).
    if (find.text('Enter address and code instead').evaluate().isNotEmpty) {
      await tap(tester, find.text('Enter address and code instead'));
      await tester.enterText(find.widgetWithText(TextField, 'Name of this phone'), 'Test desktop');
      await tester.enterText(find.widgetWithText(TextField, 'Address'), url);
      await tester.enterText(find.widgetWithText(TextField, 'Code'), code);
      await tap(tester, find.text('Pair'));
      debugPrint('smoke: pairing sent, waiting for the daemon to allow it');
    }

    // 2. Sign in (MU3-02): no names on the screen (board_members_exist only).
    //    A session kept from an earlier run (MU3-03) opens Home at once.
    final signIn = find.byKey(const ValueKey('sign-in-username'));
    final home = find.text('Home');
    await waitFor(tester, find.byWidgetPredicate((w) => w.key == const ValueKey('sign-in-username') || (w is Text && w.data == 'Home')),
        timeout: const Duration(seconds: 90), what: 'the sign-in screen or Home');
    // Home may show while the hello is still out; a refused kept session turns it into the sign-in.
    await tester.pump(const Duration(seconds: 3));
    await tester.pump(const Duration(seconds: 2));
    if (signIn.evaluate().isNotEmpty) {
      expect(find.text('robin'), findsNothing, reason: 'no list of names before a session');
      await tester.enterText(signIn, user);
      await tester.enterText(find.byKey(const ValueKey('sign-in-password')), password);
      await tap(tester, find.byKey(const ValueKey('sign-in')));
      await waitFor(tester, home, what: 'Home');
      debugPrint('smoke: signed in');
    } else {
      debugPrint('smoke: the kept session opened Home without a sign-in (MU3-03)');
    }
    await tester.pump(const Duration(seconds: 2));
    if (find.byKey(const ValueKey('claude-login-card')).evaluate().isNotEmpty) debugPrint('smoke: Home asks to connect the Claude login');

    // 3. Sessions (MU3-04): this device is listed and marked.
    go(tester, '/account/sessions');
    await waitFor(tester, find.text('This device'), what: 'the session list');
    debugPrint('smoke: the session list shows this device');

    // 4. Connect (MU3-09) with the stand-in claude: a whole code "good…#<state>".
    go(tester, '/account/claude');
    await waitFor(tester, find.byWidgetPredicate((w) => w.key == const ValueKey('claude-connect') || w.key == const ValueKey('claude-disconnect')), what: 'the Claude login page');
    if (find.byKey(const ValueKey('claude-disconnect')).evaluate().isNotEmpty) {
      // Connected by an earlier run: Disconnect first, so Connect runs again.
      await tap(tester, find.byKey(const ValueKey('claude-disconnect')));
      await tap(tester, find.descendant(of: find.byType(AlertDialog), matching: find.widgetWithText(TextButton, 'Disconnect')));
      await waitFor(tester, find.byKey(const ValueKey('claude-connect')), what: 'Not connected after Disconnect');
      debugPrint('smoke: Disconnect signed the earlier login out');
    }
    await tap(tester, find.byKey(const ValueKey('claude-connect')));
    await waitFor(tester, find.text('Copy link'), what: "Claude's sign-in link");
    // A code the stand-in refuses: the daemon's text, word for word, with Try again.
    await tester.enterText(find.byKey(const ValueKey('connect-code')), 'wrong-test-code#FAKESTATE');
    await tester.pump(const Duration(milliseconds: 200));
    await tap(tester, find.byKey(const ValueKey('connect-send')));
    await waitFor(tester, find.text('The code was not accepted. Start again.'), what: "the daemon's refusal");
    debugPrint('smoke: a wrong code shows the daemon\'s text');
    await tap(tester, find.byKey(const ValueKey('connect-again')));
    await waitFor(tester, find.text('Copy link'), what: "Claude's sign-in link again");
    await tester.enterText(find.byKey(const ValueKey('connect-code')), 'good-test-code#FAKESTATE');
    await tester.pump(const Duration(milliseconds: 200));
    await tap(tester, find.byKey(const ValueKey('connect-send')));
    await waitFor(tester, find.text('Connected'), what: 'Connected');
    await waitFor(tester, find.text('test@example.com · Max'), what: 'e-mail and plan');
    await waitFor(tester, find.byKey(const ValueKey('claude-usage')), timeout: const Duration(seconds: 20), what: 'the /usage bars');
    expect(find.textContaining('resets'), findsWidgets);
    debugPrint('smoke: connected, with the /usage bars');

    // 5. A ticket of another board member waits for their login (MU3-12); Run on my login (B4).
    if (ticket.isNotEmpty) {
      go(tester, '/ticket/$ticket');
      await waitFor(tester, find.textContaining("robin's Claude login"), what: "the wait for robin's login");
      await tap(tester, find.text('Run on my login'));
      await waitFor(tester, find.textContaining('on your Claude login?'), what: 'the confirmation');
      await tap(tester, find.descendant(of: find.byType(AlertDialog), matching: find.widgetWithText(TextButton, 'Run on my login')));
      await waitFor(tester, find.text('Your Claude login'), timeout: const Duration(seconds: 30), what: 'Runs on: Your Claude login');
      debugPrint('smoke: Run on my login moved the ticket to this login');
    }

    // 6. A ticket parked on this login's limit: the server's Paused line in Discussion (MU3-11).
    if (limitTicket.isNotEmpty) {
      go(tester, '/ticket/$limitTicket?tab=discussion');
      await waitFor(tester, find.textContaining('Paused:'), timeout: const Duration(seconds: 30), what: 'the Paused line');
      debugPrint('smoke: the Paused line shows in Discussion');
    }
  });
}
