// A1, A2 and the links of phase A against a real test daemon of F1, and the Inbox
// on a server without `personal_inbox` (AUR-FA-M2). It runs headless: no window,
// no keyring, the real app over a real socket. See integration_test/README.md for
// the daemon; robin is played by aur.py.
//
//   flutter test test/e2e/fa_m2_daemon_test.dart \
//     --dart-define=AUR_URL=http://127.0.0.1:47871 --dart-define=AUR_CODE=<pairing code> \
//     --dart-define=AUR_SOCK=<daemon socket> --dart-define=AUR_PROJECT=<project id> \
//     --dart-define=AUR_EMPLOYEE=<employee id> --dart-define=AUR_ALEX=<id> --dart-define=AUR_ROBIN=<id> \
//     --dart-define=AUR_OLD_MESSAGE=<id of the oldest message in General>
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
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/features/board/ticket_actions.dart';
import 'package:aurelius_mobile/features/links/link_actions.dart';
import 'package:aurelius_mobile/features/pairing/pairing_bloc.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
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
const oldMessage = String.fromEnvironment('AUR_OLD_MESSAGE');

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

  testWidgets('A1, A2, links and the older Inbox against the F1 daemon', (tester) async {
    HttpOverrides.global = null;
    tester.view.physicalSize = const Size(600, 2200);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    SharedPreferences.setMockInitialValues({});
    // Pairing itself is in the Phase 3 smoke run; here only its result.
    final store = MemoryHostStore();
    final pairing = PairingBloc(GatewayApi(), platform: 'ios');
    pairing.add(PairingSubmitted(PairingPayload(url: url, code: code, hostId: '', hostName: 'the PC', environment: 'dev'), deviceName: 'M2 phone'));
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

    // A1: the Responsible, and the Activity says it in the server's words.
    final made = await as('robin', 'ticket.create', {'project_id': project, 'title': 'Responsible E2E'});
    final ticket = made['ticket_id'] as String;
    final number = made['number'] as int? ?? (await as('robin', 'ticket.get', {'ticket_id': ticket}, write: false))['ticket']['number'] as int;
    go(tester, '/ticket/$ticket');
    await waitFor(tester, find.byKey(const ValueKey('ticket-make-me-responsible')), what: 'Make me responsible');
    await tap(tester, find.byKey(const ValueKey('ticket-make-me-responsible')));
    await waitFor(tester, find.text('You are now responsible for #$number.'), what: 'the toast of a handover');
    step('A1: Make me responsible went through without a version');
    // The way the `+n` of an Inbox row opens it.
    go(tester, '/home');
    await tester.pump(const Duration(milliseconds: 500));
    go(tester, '/ticket/$ticket?tab=activity');
    await waitFor(tester, find.text('alex made alex responsible for #$number'), what: 'the sentence of the server');
    expect(find.text('Nobody was responsible before'), findsWidgets);
    expect(find.text('A board member changed it'), findsNothing);
    step('A1: Activity shows "alex made alex responsible for #$number" / "Nobody was responsible before"');

    await as('robin', 'ticket.set_responsible', {'ticket_id': ticket, 'responsible_user_id': robin});
    await waitFor(tester, find.text('robin made robin responsible for #$number'), what: 'the handover by robin, live');
    expect(find.text('alex was responsible before'), findsOneWidget);
    step('A1: robin took it; live in the Activity with "alex was responsible before"');

    await as('robin', 'ticket.set_responsible', {'ticket_id': ticket, 'responsible_user_id': null});
    await waitFor(tester, find.text('robin removed robin as responsible for #$number'), what: 'the removal, live');
    expect(find.text('Nobody is responsible now'), findsOneWidget);
    step('A1: robin removed it; "Nobody is responsible now"');

    // A2: following, and the server's refusal for the Responsible.
    go(tester, '/home');
    await tester.pump(const Duration(milliseconds: 500));
    go(tester, '/ticket/$ticket');
    await waitFor(tester, find.byKey(const ValueKey('ticket-following')), what: 'the switch Following');
    await waitFor(tester, find.byKey(const ValueKey('ticket-following-note')), what: 'its reason');
    step('A2: the note says: ${(tester.widget(find.byKey(const ValueKey('ticket-following-note'))) as Text).data}');
    await as('alex', 'ticket.follow', {'ticket_id': ticket, 'on': true});
    await as('alex', 'ticket.set_responsible', {'ticket_id': ticket, 'responsible_user_id': alex});
    await waitFor(tester, find.textContaining('Hand it to someone else to stop following'), what: 'the note for the Responsible');
    step('A2: the Responsible sees why the switch is off');
    final context = pageContext(tester);
    await followTicketById(context, ticket, number, on: false);
    await waitFor(tester, find.text('You are responsible for this ticket; hand it over before you unfollow it.'), what: "the server's refusal");
    step("A2: the server refused the Unfollow, its words in the toast");
    final followers = await as('alex', 'ticket.followers', {'ticket_id': ticket}, write: false);
    expect((followers['followers'] as List).map((f) => f['username']).toSet(), containsAll(['alex', 'robin']));

    // Who is not the Responsible reads the desktop's words (freeze §5).
    await as('alex', 'ticket.set_responsible', {'ticket_id': ticket, 'responsible_user_id': robin});
    await as('alex', 'ticket.follow', {'ticket_id': ticket, 'on': false});
    await tester.pump(const Duration(seconds: 1));
    final todo = await as('robin', 'ticket.create', {'project_id': project, 'title': 'Who chooses E2E', 'responsible_user_id': robin, 'start': true});
    go(tester, '/ticket/${todo['ticket_id']}');
    await waitFor(tester, find.byKey(const ValueKey('who-responsible-strip')), what: 'the ticket of robin');
    await tester.pump(const Duration(seconds: 2));
    final shown = find.byType(Text).evaluate().map((e) => (e.widget as Text).data).whereType<String>().toSet();
    step('words for who is not the Responsible, on screen: ${shown.where((s) => s.startsWith('Waiting') || s.startsWith('Choose') || s.contains('Failed') || s.contains('choosing')).join(' | ')}');

    // A link to a message far back, on a channel of hundreds of messages, and to none.
    final repository = pageContext(tester).read<AureliusRepository>();
    go(tester, '/team');
    await tester.pump(const Duration(seconds: 1));
    await openAureliusLink(pageContext(tester), 'aurelius://${repository.host.hostId}/message/$oldMessage');
    await waitFor(tester, find.byKey(const ValueKey('flash-$oldMessage')), timeout: const Duration(seconds: 90), what: 'the oldest message, marked');
    expect(find.text('That message no longer exists.'), findsNothing);
    step('links: the oldest of 700+ messages opens and is marked; not called gone');
    await tester.pump(const Duration(seconds: 2));
    go(tester, '/team');
    await tester.pump(const Duration(seconds: 1));
    await openAureliusLink(pageContext(tester), 'aurelius://${repository.host.hostId}/message/01a19c13-3944-7338-8824-1be479d451ff');
    await waitFor(tester, find.text('That message no longer exists.'), what: 'the not found of the server');
    step('links: an unknown message is called gone, by the server');

    // This server has no personal_inbox: the Inbox as it was.
    expect(repository.linkState.has('personal_inbox'), isFalse);
    go(tester, '/inbox');
    await waitFor(tester, find.byKey(const ValueKey('inbox-section-needs-you')), what: 'the Inbox');
    expect(find.byKey(const ValueKey('inbox-section-updates')), findsNothing);
    expect(find.byKey(const ValueKey('inbox-section-running')), findsOneWidget);
    expect(find.byKey(const ValueKey('inbox-section-done')), findsOneWidget);
    step('older server: Needs you, Running, Done, no Updates');
  }, skip: url.isEmpty);
}
