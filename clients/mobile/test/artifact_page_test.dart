import 'dart:convert';
import 'dart:typed_data';

import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/discussion.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/artifact/artifact_bloc.dart';
import 'package:aurelius_mobile/features/artifact/artifact_page.dart';
import 'package:aurelius_mobile/features/artifact/artifact_versions.dart';
import 'package:aurelius_mobile/features/artifact/artifact_webview.dart';
import 'package:aurelius_mobile/features/board/ticket_page.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

// Ticket #79 phase C: a kept text/html file opens the Artifact page, with its
// versions newest first, three widths, Reload, Share HTML and Ask for changes.

const _link = LinkState(
  status: LinkStatus.online,
  capabilities: ['read', 'chat', 'task_control', 'approve'],
  features: [ticketDiscussionFeature],
  boardMembersExist: true,
  user: testUser,
);

FileRef html(String id) => FileRef(id: id, mediaType: 'text/html', name: 'pricing.html', size: 2048);

MessageView answerWith(String id, int ordinal, List<FileRef> files, {int at = 0}) => MessageView(
      activity: const [],
      author: 'Forge',
      createdAtMs: at,
      files: files,
      id: id,
      ordinal: ordinal,
      queued: false,
      role: MessageRole.assistant,
      sources: const [],
      status: MessageStatus.complete,
      text: 'Saved design/pricing.html ($id)',
      uploads: const [],
    );

Uint8List bytesOf(String s) => Uint8List.fromList(utf8.encode(s));

/// What the stage was asked to show, in place of the platform webview.
final shown = <ArtifactState>[];

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  setUp(() {
    shown.clear();
    artifactStage = (state) {
      shown.add(state);
      return Text('stage ${state.current.file.id} load ${state.load} ${state.viewport.label}', key: const Key('fake-stage'));
    };
  });

  group('ArtifactBloc', () {
    late MockRepository repo;
    var nonces = 0;
    String nextNonce() => (++nonces).toRadixString(16).padLeft(32, '0');

    setUp(() {
      repo = MockRepository();
      nonces = 0;
      when(() => repo.file('f1')).thenAnswer((_) async => bytesOf('<p>one</p><script>a()</script>'));
      when(() => repo.file('f2')).thenAnswer((_) async => bytesOf('<p>two</p>'));
    });

    ArtifactBloc open(String fileId) => ArtifactBloc(
          repo,
          file: html(fileId),
          messages: [answerWith('m1', 1, [html('f1')], at: 10), answerWith('m2', 2, [html('f2')], at: 20)],
          openedAtMs: 0,
          nonce: nextNonce,
        );

    test('opens on the version tapped, with the versions newest first', () async {
      final bloc = open('f1');
      addTearDown(bloc.close);
      final ready = await stateWhere(bloc, (s) => s.document != null);
      expect([for (final v in ready.versions) (v.version, v.file.id)], [(2, 'f2'), (1, 'f1')]);
      expect(ready.current.version, 1);
      expect(ready.html, '<p>one</p><script>a()</script>');
      final nonce = '1'.padLeft(32, '0');
      expect(ready.document, contains("script-src 'nonce-$nonce'"));
      expect(ready.document, contains('<script nonce=$nonce>a()'));
    });

    test('Reload loads the same HTML again with a fresh nonce, without asking the server again', () async {
      final bloc = open('f1');
      addTearDown(bloc.close);
      final first = await stateWhere(bloc, (s) => s.document != null);
      bloc.add(const ArtifactReloaded());
      final again = await stateWhere(bloc, (s) => s.load > first.load);
      expect(again.document, isNot(first.document));
      expect(again.document, contains("'nonce-${'2'.padLeft(32, '0')}'"));
      verify(() => repo.file('f1')).called(1);
    });

    test('another version is fetched and shown', () async {
      final bloc = open('f1');
      addTearDown(bloc.close);
      await stateWhere(bloc, (s) => s.document != null);
      bloc.add(const ArtifactVersionSelected('f2'));
      final two = await stateWhere(bloc, (s) => s.fileId == 'f2' && s.document != null);
      expect(two.current.version, 2);
      expect(two.html, '<p>two</p>');
    });

    test('a failure says what failed, and Try again asks the server again', () async {
      when(() => repo.file('f1')).thenThrow(const DisconnectedException('Not connected to the PC.'));
      final bloc = open('f1');
      addTearDown(bloc.close);
      final failed = await stateWhere(bloc, (s) => s.error != null);
      expect(failed.error, 'Not connected to the PC.');
      expect(failed.document, isNull);
      when(() => repo.file('f1')).thenAnswer((_) async => bytesOf('<p>back</p>'));
      bloc.add(const ArtifactRetried());
      final back = await stateWhere(bloc, (s) => s.html == '<p>back</p>');
      expect(back.error, isNull);
      verify(() => repo.file('f1')).called(2);
    });

    test('a new version in the conversation joins the list at the top', () async {
      final bloc = open('f2');
      addTearDown(bloc.close);
      await stateWhere(bloc, (s) => s.document != null);
      bloc.add(ArtifactMessagesChanged([
        answerWith('m1', 1, [html('f1')]),
        answerWith('m2', 2, [html('f2')]),
        answerWith('m3', 3, [html('f3')]),
      ]));
      final more = await stateWhere(bloc, (s) => s.versions.length == 3);
      expect(more.versions.first.file.id, 'f3');
      expect(more.fileId, 'f2');
    });
  });

  group('navigation', () {
    test('only the document the page loads may navigate, once', () {
      expect(artifactNavigationAllowed(url: 'about:blank', isMainFrame: true, expectingLoad: true), isTrue);
      expect(artifactNavigationAllowed(url: 'about:blank', isMainFrame: true, expectingLoad: false), isFalse);
      expect(artifactNavigationAllowed(url: 'about:blank', isMainFrame: false, expectingLoad: true), isFalse);
      for (final url in ['https://example.invalid/', 'http://127.0.0.1:9/', 'javascript:alert(1)', 'data:text/html,x', 'file:///etc/hosts', 'about:srcdoc']) {
        expect(artifactNavigationAllowed(url: url, isMainFrame: true, expectingLoad: true), isFalse, reason: url);
      }
    });
  });

  group('Artifact page in a ticket discussion', () {
    late MockRepository repo;
    final saved = <(String, String)>[];

    Future<void> open(WidgetTester tester) async {
      final ticket = TicketView.fromJson({
        ...ticketOf(column: ColumnKind.doing, ownerEmployeeId: 'forge', ownerName: 'Forge', rootTaskId: 'root').toJson(),
        'conversation_id': 'disc-1',
      });
      repo = boardRepository(
        detail: TicketDetail(asOfSeq: 1, ticket: ticket, tasks: const []),
        messages: snapshotOf('disc-1', messages: [
          answerWith('m1', 1, [html('f1')], at: DateTime(2026, 11, 22, 9, 30).millisecondsSinceEpoch),
          answerWith('m2', 2, [html('f2')], at: DateTime(2026, 11, 22, 14, 5).millisecondsSinceEpoch),
        ]),
        link: _link,
      );
      when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.appends).thenAnswer((_) => const Stream.empty());
      when(() => repo.loadChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.load()).thenAnswer((_) async => loadOf());
      when(() => repo.boardMembers()).thenAnswer((_) async => const [testUser]);
      when(() => repo.file('f1')).thenAnswer((_) async => bytesOf('<p>v1</p>'));
      when(() => repo.file('f2')).thenAnswer((_) async => bytesOf('<!doctype html><p>v2</p>'));
      saved.clear();
      artifactSave = (name, bytes) async {
        saved.add((name, utf8.decode(bytes)));
        return Uri.parse('content://saved/$name');
      };
      await pumpBoardPage(tester, repo, TicketPage(ticketId: ticket.id, discussion: true), link: _link);
      // The chat lists the newest message first in its tree (a reversed list).
      await tester.tap(find.textContaining('pricing.html ·').first);
      await settleBoard(tester);
    }

    testWidgets('the HTML chip opens the newest version with its toolbar', (tester) async {
      await open(tester);
      expect(find.text('pricing.html'), findsOneWidget);
      expect(find.text('v2 · ${versionLabel(ArtifactVersion(version: 2, file: html('f2'), messageId: 'm2', atMs: DateTime(2026, 11, 22, 14, 5).millisecondsSinceEpoch), DateTime.now()).split(' · ').last}'), findsOneWidget);
      expect(find.byTooltip('Reload'), findsOneWidget);
      expect(find.byTooltip('Share HTML'), findsOneWidget);
      for (final w in ['Desktop', 'Tablet', 'Mobile']) {
        expect(find.text(w), findsOneWidget);
      }
      expect(find.text('Ask for changes'), findsOneWidget);
      expect(find.text('stage f2 load 1 Mobile'), findsOneWidget);
      // What the webview gets: the artifact after its doctype, behind the policy.
      expect(shown.last.document, startsWith('<!doctype html><meta http-equiv="Content-Security-Policy"'));
    });

    testWidgets('the version menu lists the versions newest first and switches', (tester) async {
      await open(tester);
      await tester.tap(find.byKey(const Key('artifact-version')));
      await settleBoard(tester);
      final labels = tester.widgetList<Text>(find.textContaining(RegExp(r'^v\d · '))).map((t) => t.data!).toList();
      expect(labels.where((l) => l.startsWith('v2')).length, greaterThanOrEqualTo(1));
      expect(labels.indexWhere((l) => l.startsWith('v2')), lessThan(labels.lastIndexWhere((l) => l.startsWith('v1'))));
      await tester.tap(find.textContaining('v1 · ').last);
      await settleBoard(tester);
      expect(find.text('stage f1 load 2 Mobile'), findsOneWidget);
    });

    testWidgets('the width toggle and Reload reach the stage', (tester) async {
      await open(tester);
      await tester.tap(find.text('Desktop'));
      await settleBoard(tester);
      expect(find.text('stage f2 load 1 Desktop'), findsOneWidget);
      await tester.tap(find.byTooltip('Reload'));
      await settleBoard(tester);
      expect(find.text('stage f2 load 2 Desktop'), findsOneWidget);
    });

    testWidgets('Share HTML saves the shown version as the server keeps it', (tester) async {
      await open(tester);
      await tester.tap(find.byTooltip('Share HTML'));
      await settleBoard(tester);
      expect(saved, [('pricing.html', '<!doctype html><p>v2</p>')]);
      expect(find.text('Saved pricing.html.'), findsOneWidget);
    });

    testWidgets('Ask for changes posts one message in the discussion it came from', (tester) async {
      await open(tester);
      await tester.tap(find.text('Ask for changes'));
      await settleBoard(tester);
      expect(find.text('What should change in pricing.html v2?'), findsOneWidget);
      await tester.enterText(find.descendant(of: find.byType(AlertDialog), matching: find.byType(TextField)), 'Make the Team plan stand out.');
      await tester.pump();
      await tester.tap(find.descendant(of: find.byType(AlertDialog), matching: find.text('Send')));
      await settleBoard(tester);
      final posts = writesOf(repo).where((w) => w['method'] == 'ticket.post').toList();
      expect(posts, hasLength(1));
      expect(paramsOf(posts.single)['ticket_id'], 'k1');
      expect(paramsOf(posts.single)['text'], 'Feedback on pricing.html v2: Make the Team plan stand out.');
      expect(find.text('Sent to the conversation.'), findsOneWidget);
    });

    testWidgets('a load that fails says so and offers Try again', (tester) async {
      await open(tester);
      await tester.pageBack();
      await settleBoard(tester);
      when(() => repo.file('f1')).thenThrow(const ProtocolException(ErrorCode.notFound, 'That file is gone.'));
      await tester.tap(find.textContaining('pricing.html ·').last);
      await settleBoard(tester);
      expect(find.text('This artifact could not be loaded: That file is gone.'), findsOneWidget);
      when(() => repo.file('f1')).thenAnswer((_) async => bytesOf('<p>v1</p>'));
      await tester.tap(find.text('Try again'));
      await settleBoard(tester);
      expect(find.textContaining('stage f1'), findsOneWidget);
    });
  });
}
