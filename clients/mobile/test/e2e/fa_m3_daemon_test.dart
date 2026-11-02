// A4 (My tickets) and A6 (edit and delete a message) against a real test daemon
// with F3 (AUR-FA-M4, the E2E of M3). It runs headless: no window,
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
import 'package:aurelius_mobile/features/pairing/pairing_bloc.dart';
import 'package:flutter/gestures.dart';
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
Future<Map<String, dynamic>> as(String who, String method, Map<String, dynamic> params, {bool write = true}) async {
  final done = await Process.run('python3', ['-I', 'integration_test/daemon/aur.py', sock, who, who == 'alex' ? 'test-password-1' : 'test-password-2', method, jsonEncode(params), if (write) '--write']);
  final answer = jsonDecode(done.stdout as String) as Map<String, dynamic>;
  if (answer['ok'] == null) throw StateError('$method: ${answer['error']}');
  final value = (answer['ok'] as Map<String, dynamic>)['value'] as Map<String, dynamic>;
  return write ? ((value['result'] as Map<String, dynamic>)['value'] as Map<String, dynamic>) : value;
}

void main() {
  LiveTestWidgetsFlutterBinding.ensureInitialized();

  Future<void> waitFor(WidgetTester tester, Finder finder, {Duration timeout = const Duration(seconds: 30), String? what}) async {
    final end = DateTime.now().add(timeout);
    while (DateTime.now().isBefore(end)) {
      await tester.pump(const Duration(milliseconds: 100));
      if (finder.evaluate().isNotEmpty) return;
    }
    final shown = find.byType(Text).evaluate().map((e) => (e.widget as Text).data ?? (e.widget as Text).textSpan?.toPlainText()).whereType<String>().toSet();
    fail('not seen within $timeout: ${what ?? finder}; on screen: ${shown.join(' | ')}');
  }

  Future<void> tap(WidgetTester tester, Finder finder) async {
    await waitFor(tester, finder);
    await tester.ensureVisible(finder.first);
    await tester.pump(const Duration(milliseconds: 200));
    await tester.tap(finder.first, warnIfMissed: false);
    await tester.pump(const Duration(milliseconds: 300));
  }

  BuildContext pageContext(WidgetTester tester) => tester.element(find.byType(Scaffold).last);
  void go(WidgetTester tester, String location) => GoRouter.of(pageContext(tester)).go(location);
  void step(String text) => debugPrint('e2e: $text');

  Future<void> longPress(WidgetTester tester, String text) async {
    final gesture = await tester.startGesture(tester.getCenter(find.text(text).first));
    await tester.pump(kLongPressTimeout + const Duration(milliseconds: 100));
    await gesture.up();
    await tester.pump(const Duration(milliseconds: 500));
  }

  /// The menu row [label] as the app's own InkWell takes it (a tap in the live binding does not reach a popup route).
  Future<void> choose(WidgetTester tester, String label) async {
    final row = find.ancestor(of: find.text(label), matching: find.byType(InkWell)).first;
    tester.widget<InkWell>(row).onTap!();
    await tester.pump(const Duration(milliseconds: 800));
  }

  testWidgets('A4 and A6 against the F3 daemon', (tester) async {
    HttpOverrides.global = null;
    tester.view.physicalSize = const Size(600, 2200);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    SharedPreferences.setMockInitialValues({});
    final store = MemoryHostStore();
    final pairing = PairingBloc(GatewayApi(), platform: 'ios');
    pairing.add(PairingSubmitted(PairingPayload(url: url, code: code, hostId: '', hostName: 'the PC', environment: 'dev'), deviceName: 'M3 phone'));
    final paired = await pairing.stream.firstWhere((s) => s.step == PairingStep.paired || s.step == PairingStep.failed).timeout(const Duration(seconds: 60));
    await pairing.close();
    if (paired.host == null) fail('pairing: ${paired.message}');
    await store.save(paired.host!, paired.deviceSecret!);
    await store.setActive(paired.host!.hostId);
    await tester.pumpWidget(AppearanceScope(child: AureliusApp(store: store, api: GatewayApi(), push: PushService())));
    await tester.pump(const Duration(seconds: 1));
    await waitFor(tester, find.byKey(const ValueKey('sign-in-username')), timeout: const Duration(seconds: 90), what: 'the sign-in');
    await tester.enterText(find.byKey(const ValueKey('sign-in-username')), 'alex');
    await tester.enterText(find.byKey(const ValueKey('sign-in-password')), 'test-password-1');
    await tap(tester, find.byKey(const ValueKey('sign-in')));
    await waitFor(tester, find.text('Home'), what: 'Home');
    await tester.pump(const Duration(seconds: 2));
    step('paired and signed in as alex');

    // A4: one ticket for each tab.
    await as('robin', 'ticket.create', {'project_id': project, 'title': 'E2E responsible', 'responsible_user_id': alex});
    await as('alex', 'ticket.create', {'project_id': project, 'title': 'E2E created by alex'});
    final followed = await as('robin', 'ticket.create', {'project_id': project, 'title': 'E2E followed'});
    await as('alex', 'ticket.follow', {'ticket_id': followed['ticket_id'], 'on': true});
    go(tester, '/board');
    await waitFor(tester, find.byKey(const ValueKey('board-switch')), what: 'the Board | My tickets switch');
    await tap(tester, find.text('My tickets'));
    await waitFor(tester, find.byKey(const ValueKey('my-tab-responsible')), what: 'the tabs');
    await waitFor(tester, find.byKey(const ValueKey('my-tab-waiting')), what: 'the tabs');
    step('A4: first tab is Waiting on me while something waits (the ticket on alex\'s login)');
    await tap(tester, find.byKey(const ValueKey('my-tab-responsible')));
    await waitFor(tester, find.text('E2E responsible'), what: 'the Responsible ticket');
    step('A4: switch shown, Responsible tab lists "E2E responsible"');
    await tap(tester, find.byKey(const ValueKey('my-tab-created')));
    await waitFor(tester, find.text('E2E created by alex'), what: 'Created by me');
    expect(find.text('E2E responsible'), findsNothing);
    step('A4: Created by me lists only alex\'s own');
    await tap(tester, find.byKey(const ValueKey('my-tab-following')));
    await waitFor(tester, find.text('E2E followed'), what: 'Following');
    step('A4: Following lists the followed ticket');
    await tap(tester, find.byKey(const ValueKey('my-tab-waiting')));
    await tester.pump(const Duration(seconds: 1));
    step('A4: Waiting on me, on screen: ${find.byType(Text).evaluate().map((e) => (e.widget as Text).data).whereType<String>().where((s) => s.startsWith('Nothing') || s.startsWith('E2E')).join(' | ')}');
    await as('robin', 'ticket.set_responsible', {'ticket_id': followed['ticket_id'], 'responsible_user_id': alex});
    await tap(tester, find.byKey(const ValueKey('my-tab-responsible')));
    await waitFor(tester, find.text('E2E followed'), what: 'live: the ticket robin handed to alex');
    step('A4: a handover by robin shows up live in Responsible');

    // A6: robin's message cannot be edited by alex; alex's can, until deleted.
    await as('robin', 'group_chat.send', {'text': 'robin says hello'});
    await as('alex', 'group_chat.send', {'text': 'alex first draft'});
    go(tester, '/channel/dde88fb3-3a20-447a-a9db-c04e265ce8c4');
    await waitFor(tester, find.text('alex first draft'), what: 'alex message in General');
    await longPress(tester, 'robin says hello');
    expect(find.text('Edit'), findsNothing);
    expect(find.text('Delete'), findsNothing);
    step("A6: robin's message offers no Edit or Delete");
    await tester.tapAt(const Offset(10, 10));
    await tester.pump(const Duration(milliseconds: 500));
    await longPress(tester, 'alex first draft');
    await tester.pump(const Duration(milliseconds: 500));
    await choose(tester, 'Edit');
    await waitFor(tester, find.byKey(const ValueKey('message-edit-field')), what: 'the edit sheet');
    await tester.enterText(find.byKey(const ValueKey('message-edit-field')), 'alex second draft');
    await tap(tester, find.byKey(const ValueKey('message-edit-save')));
    await waitFor(tester, find.text('alex second draft'), what: 'the edited text');
    await waitFor(tester, find.text('edited'), what: 'the edited mark');
    step('A6: edit saved, "edited" shown');
    tester.widget<InkWell>(find.ancestor(of: find.text('edited'), matching: find.byType(InkWell)).first).onTap!();
    await tester.pump(const Duration(milliseconds: 800));
    await waitFor(tester, find.text('Earlier versions'), what: 'earlier versions');
    expect(find.text('alex first draft'), findsOneWidget);
    step('A6: tapping "edited" shows the earlier version');
    step('versions open: ${find.text('Earlier versions').evaluate().length}; text: ${find.text('alex second draft').evaluate().length}');
    if (find.text('Earlier versions').evaluate().isNotEmpty) Navigator.of(tester.element(find.text('Earlier versions')), rootNavigator: true).pop();
    await tester.pump(const Duration(seconds: 1));
    await tester.pump(const Duration(seconds: 1));
    await tester.pump(const Duration(milliseconds: 500));
    await as('alex', 'group_chat.send', {'text': 'alex to delete'});
    await waitFor(tester, find.text('alex to delete'), what: 'the message to delete');
    await longPress(tester, 'alex to delete');
    await choose(tester, 'Delete');
    await waitFor(tester, find.byKey(const ValueKey('message-delete-confirm')), what: 'the delete dialog');
    await tap(tester, find.byKey(const ValueKey('message-delete-confirm')));
    await waitFor(tester, find.text('This message was deleted.'), what: 'the placeholder');
    expect(find.text('alex to delete'), findsNothing);
    step('A6: delete leaves "This message was deleted." and no text');
  }, skip: url.isEmpty);
}
