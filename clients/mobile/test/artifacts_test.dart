import 'dart:async';

import 'package:aurelius_mobile/app/mobile_core.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/artifacts/artifact_sheet.dart';
import 'package:aurelius_mobile/features/artifacts/artifacts.dart';
import 'package:aurelius_mobile/features/chat/answer_body.dart';
import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

class MockRepository extends Mock implements AureliusRepository {}

const approver = LinkState(status: LinkStatus.online, capabilities: ['read', 'approve']);
const reader = LinkState(status: LinkStatus.online, capabilities: ['read']);

const url = 'https://claude.ai/artifact/2Jwgr8SC4Y55s2rme4hyfb';
const publishedOutput = 'Published C:\\Temp\\claude\\x\\scratchpad\\spike.html at $url (Version 2, version id 1791545049-8abb) Icon: "page".';

ActivityView publish({String? output = publishedOutput, bool? ok = true, Map<String, dynamic>? input}) => ActivityView(
      callId: 'c1',
      input: input ?? {'file_path': 'C:\\Temp\\claude\\x\\scratchpad\\spike.html', 'icon': 'page', 'description': 'A test page'},
      offset: 0,
      ok: ok,
      output: output,
      tool: 'Artifact',
    );

ArtifactView view(String title, {int version = 1, String link = url, bool kept = false}) => ArtifactView(
    conversationId: 'c', firstPublishedAtMs: 0, lastPublishedAtMs: DateTime.now().millisecondsSinceEpoch, title: title, url: link, version: version, hasContent: kept);

ArtifactContent content(ArtifactKind kind, String text, {int version = 2, List<int> versions = const [2, 1], bool snapshot = true}) => ArtifactContent(
      artifact: view('Q3 report', version: version, kept: true),
      versions: versions,
      snapshot: snapshot ? ArtifactSnapshot(content: text, fileName: 'q3.html', kind: kind, publishedAtMs: 0, version: version) : null,
    );

Widget app(AureliusRepository repo, Widget child) =>
    RepositoryProvider<AureliusRepository>.value(value: repo, child: MaterialApp(theme: aureliusTheme(), home: child));

void main() {
  late List<Uri> launched;
  late bool launchWorks;
  late List<Uri> inApp;
  late bool inAppWorks;
  String? copied;

  setUp(() {
    launched = [];
    launchWorks = true;
    inApp = [];
    inAppWorks = true;
    copied = null;
    launchExternal = (uri) async {
      launched.add(uri);
      return launchWorks;
    };
    launchInAppBrowser = (uri) async {
      inApp.add(uri);
      return inAppWorks;
    };
  });

  void captureClipboard(WidgetTester tester) {
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
      if (call.method == 'Clipboard.setData') copied = (call.arguments as Map)['text'] as String;
      return null;
    });
    addTearDown(() => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, null));
  }

  group('publish calls', () {
    test('a finished publish gives its URL, title and version', () {
      final a = artifactPublished(publish())!;
      expect(a.url, url);
      expect(a.title, 'spike');
      expect(a.version, 2);
    });

    test('a title argument wins over the file name', () {
      expect(artifactTitle(publish(input: {'file_path': 'a/b.html', 'title': 'Report'})), 'Report');
    });

    test('running, failed and non-publish calls give no card', () {
      expect(artifactPublished(publish(ok: null, output: null)), isNull);
      expect(artifactPublished(publish(ok: false, output: 'denied')), isNull);
      expect(artifactPublished(publish(input: {'action': 'list'})), isNull);
    });

    test('a republish without a URL in the text uses the url argument', () {
      final a = artifactPublished(publish(output: 'done', input: {'file_path': 'p.html', 'url': url}))!;
      expect(a.url, url);
      expect(a.version, isNull);
    });
  });

  group('activity card', () {
    // The artifact card is hidden in the mobile core (ticket #19); these tests look at the card itself.
    setUp(() => mobileCoreHidden = const {});
    tearDown(() => mobileCoreHidden = kHiddenInMobileCore);

    testWidgets('shows the artifact with Open and Copy link', (tester) async {
      captureClipboard(tester);
      await tester.pumpWidget(MaterialApp(theme: aureliusTheme(), home: Scaffold(body: ActivityCard(activity: publish()))));
      expect(find.text('Published an artifact: spike'), findsOneWidget);
      expect(find.byKey(const Key('artifact-card')), findsOneWidget);
      expect(find.textContaining('version 2'), findsOneWidget);
      await tester.tap(find.byKey(const Key('artifact-open')));
      await tester.pump();
      expect(inApp, [Uri.parse(url)], reason: 'claude.ai opens in the in-app browser');
      expect(launched, isEmpty);
      await tester.tap(find.byKey(const Key('artifact-copy')));
      await tester.pump();
      expect(copied, url);
      await tester.pump(const Duration(seconds: 2));
    });

    testWidgets('a link that cannot open is copied instead', (tester) async {
      captureClipboard(tester);
      inAppWorks = false;
      launchWorks = false;
      await tester.pumpWidget(MaterialApp(theme: aureliusTheme(), home: Scaffold(body: ActivityCard(activity: publish()))));
      await tester.tap(find.byKey(const Key('artifact-open')));
      await tester.pumpAndSettle();
      expect(copied, url);
      expect(find.text('Could not open the link; it is copied.'), findsOneWidget);
    });

    testWidgets('the card shows the title from the Artifacts list, then the call title', (tester) async {
      final repo = MockRepository();
      final changes = StreamController<Event>.broadcast();
      addTearDown(changes.close);
      when(() => repo.changes).thenAnswer((_) => changes.stream);
      ArtifactView? known;
      when(() => repo.knownArtifact(any(), fresh: any(named: 'fresh'))).thenAnswer((_) async => known);
      await tester.pumpWidget(app(repo, Scaffold(body: ActivityCard(activity: publish()))));
      await tester.pump();
      // Not recorded yet: the file name.
      expect(find.descendant(of: find.byKey(const Key('artifact-card')), matching: find.text('spike')), findsOneWidget);

      known = view('Q3 revenue');
      changes.add(const Event(aggregateId: 'c', atMs: 0, kind: EventKind.artifactPublished, seq: 13));
      await tester.pump();
      await tester.pump();
      expect(find.descendant(of: find.byKey(const Key('artifact-card')), matching: find.text('Q3 revenue')), findsOneWidget);
      verify(() => repo.knownArtifact(url, fresh: true)).called(1);
    });

    testWidgets('when the in-app browser fails, the browser opens it', (tester) async {
      inAppWorks = false;
      await tester.pumpWidget(MaterialApp(theme: aureliusTheme(), home: Scaffold(body: ActivityCard(activity: publish()))));
      await tester.tap(find.byKey(const Key('artifact-open')));
      await tester.pump();
      expect(inApp, [Uri.parse(url)]);
      expect(launched, [Uri.parse(url)]);
    });

    testWidgets('other links skip the in-app browser', (tester) async {
      await tester.pumpWidget(MaterialApp(
        theme: aureliusTheme(),
        home: Scaffold(body: Builder(builder: (context) => TextButton(onPressed: () => openExternalLink(context, 'https://example.com/x'), child: const Text('go')))),
      ));
      await tester.tap(find.text('go'));
      await tester.pump();
      expect(inApp, isEmpty);
      expect(launched, [Uri.parse('https://example.com/x')]);
    });

    test('claude.ai links', () {
      expect(isClaudeAiLink(Uri.parse(url)), isTrue);
      expect(isClaudeAiLink(Uri.parse(claudeAiLoginUrl)), isTrue);
      expect(isClaudeAiLink(Uri.parse('http://claude.ai/login')), isFalse);
      expect(isClaudeAiLink(Uri.parse('https://notclaude.ai/x')), isFalse);
    });

    testWidgets('no card while publishing', (tester) async {
      await tester.pumpWidget(MaterialApp(theme: aureliusTheme(), home: Scaffold(body: ActivityCard(activity: publish(ok: null, output: null)))));
      expect(find.text('Publishing an artifact: spike'), findsOneWidget);
      expect(find.byKey(const Key('artifact-card')), findsNothing);
    });
  });

  group('settings', () {
    late MockRepository repo;

    Future<void> pump(WidgetTester tester, LinkState link, {Future<ArtifactSetting> Function()? get}) async {
      repo = MockRepository();
      when(() => repo.linkState).thenReturn(link);
      when(() => repo.artifactSetting(any())).thenAnswer((_) => get == null ? Future.value(const ArtifactSetting(enabled: false)) : get());
      await tester.pumpWidget(app(repo, const Scaffold(body: SingleChildScrollView(child: ArtifactsStatus()))));
      await tester.pump();
    }

    setUpAll(() => registerFallbackValue(Duration.zero));

    testWidgets('Advanced shows the artifacts setting read-only, whatever the rights', (tester) async {
      await pump(tester, approver, get: () => Future.value(const ArtifactSetting(enabled: true)));
      expect(find.text('CLAUDE ARTIFACTS'), findsOneWidget);
      expect(find.text('On'), findsOneWidget);
      expect(find.byType(CupertinoSwitch), findsNothing);
    });

    testWidgets('off is shown as Off', (tester) async {
      await pump(tester, reader);
      expect(find.text('Off'), findsOneWidget);
      expect(find.byType(CupertinoSwitch), findsNothing);
    });

    testWidgets('servers without artifacts show nothing', (tester) async {
      await pump(tester, approver, get: () => Future.error(TimeoutException('no answer')));
      expect(find.byKey(const ValueKey('artifacts-section')), findsNothing);
      expect(find.text('CLAUDE ARTIFACTS'), findsNothing);
    });

    testWidgets('claude.ai account: Sign in opens the login in the in-app browser', (tester) async {
      await tester.pumpWidget(MaterialApp(theme: aureliusTheme(), home: const Scaffold(body: SingleChildScrollView(child: ClaudeAiAccountSection()))));
      expect(find.text('claude.ai account'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('claude-ai-sign-in')));
      await tester.pump();
      expect(inApp, [Uri.parse(claudeAiLoginUrl)]);
      expect(launched, isEmpty);
    });
  });

  group('in-app view', () {
    setUp(() => mobileCoreHidden = const {});
    tearDown(() => mobileCoreHidden = kHiddenInMobileCore);

    test('HTML gets a CSP that blocks connections, in its head', () {
      expect(withArtifactCsp('<!doctype html><html><head><title>x</title></head><body></body></html>'), contains('<head><meta http-equiv="Content-Security-Policy"'));
      expect(withArtifactCsp('<!DOCTYPE html><HTML lang="en"><body>x</body></HTML>'), contains('<HTML lang="en"><head><meta'));
      expect(withArtifactCsp('<!doctype html><p>x</p>'), startsWith('<!doctype html><meta'));
      expect(withArtifactCsp('<p>x</p>'), startsWith('<meta'));
      expect(withArtifactCsp('<p>x</p>'), contains("connect-src 'none'"));
    });

    test('the page stays put; only http(s) and mailto links go out', () {
      expect(artifactNavigationStays('about:blank'), isTrue);
      expect(artifactNavigationStays('https://example.com'), isFalse);
      expect(isExternalArtifactLink('https://example.com/a'), isTrue);
      expect(isExternalArtifactLink('mailto:a@b.c'), isTrue);
      expect(isExternalArtifactLink('file:///etc/passwd'), isFalse);
      expect(isExternalArtifactLink('intent://x#Intent;end'), isFalse);
    });

    MockRepository repoWith(ArtifactContent Function(int? version) answer, {ArtifactView? known}) {
      final repo = MockRepository();
      when(() => repo.changes).thenAnswer((_) => const Stream<Event>.empty());
      when(() => repo.online).thenAnswer((_) => const Stream<void>.empty());
      when(() => repo.knownArtifact(any(), fresh: any(named: 'fresh'))).thenAnswer((_) async => known);
      when(() => repo.artifactContent(any(), version: any(named: 'version'))).thenAnswer((i) async => answer(i.namedArguments[#version] as int?));
      return repo;
    }

    Widget opener(ArtifactView artifact) =>
        Builder(builder: (context) => Scaffold(body: TextButton(onPressed: () => openArtifact(context, artifact), child: const Text('go'))));

    testWidgets('the card takes the listed title and opens the sheet with Markdown', (tester) async {
      final repo = repoWith((_) => content(ArtifactKind.markdown, '# Heading\n\nSome **text**.'), known: view('Q3 report', version: 2, kept: true));
      await tester.pumpWidget(app(repo, Scaffold(body: ActivityCard(activity: publish()))));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('artifact-title')), findsOneWidget);
      expect(tester.widget<Text>(find.byKey(const Key('artifact-title'))).data, 'Q3 report', reason: 'not the file name');
      await tester.tap(find.byKey(const Key('artifact-view')));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('artifact-sheet')), findsOneWidget);
      expect(find.byKey(const Key('artifact-markdown')), findsOneWidget);
      expect(find.text('Heading'), findsOneWidget);
      expect(find.byKey(const ValueKey('artifact-version-1')), findsOneWidget);
      await tester.tap(find.byKey(const Key('artifact-sheet-open')));
      await tester.pump();
      expect(inApp, [Uri.parse(url)]);
      await tester.tap(find.byKey(const Key('artifact-sheet-close')));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('artifact-sheet')), findsNothing);
    });

    testWidgets('a card without a kept file keeps the link only', (tester) async {
      final repo = repoWith((_) => content(ArtifactKind.html, ''), known: view('Old page', version: 2));
      await tester.pumpWidget(app(repo, Scaffold(body: ActivityCard(activity: publish()))));
      await tester.pumpAndSettle();
      expect(find.text('Old page'), findsOneWidget);
      expect(find.byKey(const Key('artifact-view')), findsNothing);
      expect(find.byKey(const Key('artifact-open')), findsOneWidget);
    });

    testWidgets('HTML goes to the webview; picking an older version loads it', (tester) async {
      final asked = <int?>[];
      artifactHtmlView = (html) => Text('webview: $html');
      addTearDown(() => artifactHtmlView = (html) => ArtifactWebView(html: html));
      final repo = repoWith((v) {
        asked.add(v);
        return v == 1 ? content(ArtifactKind.html, '<p>one</p>', version: 1) : content(ArtifactKind.html, '<p>two</p>');
      });
      await tester.pumpWidget(app(repo, opener(view('Q3 report', version: 2, kept: true))));
      await tester.tap(find.text('go'));
      await tester.pumpAndSettle();
      expect(find.text('webview: <p>two</p>'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('artifact-version-1')));
      await tester.pumpAndSettle();
      expect(find.text('webview: <p>one</p>'), findsOneWidget);
      expect(asked, [null, 1]);
    });

    testWidgets('no kept copy or a deleted conversation says so', (tester) async {
      final repo = repoWith((_) => content(ArtifactKind.html, '', snapshot: false));
      await tester.pumpWidget(app(repo, opener(view('Q3 report', kept: true))));
      await tester.tap(find.text('go'));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('artifact-sheet-none')), findsOneWidget);
      await tester.tap(find.byKey(const Key('artifact-sheet-close')));
      await tester.pumpAndSettle();

      when(() => repo.artifactContent(any(), version: any(named: 'version'))).thenAnswer((_) async => throw const ProtocolException(ErrorCode.notFound, 'gone'));
      await tester.tap(find.text('go'));
      await tester.pumpAndSettle();
      expect(find.textContaining('no longer on the server'), findsOneWidget);
    });
  });
}
