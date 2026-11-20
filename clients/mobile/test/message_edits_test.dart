import 'dart:async';

import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/discussion.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/ticket_page.dart';
import 'package:aurelius_mobile/features/chat/message_edit.dart';
import 'package:flutter/gestures.dart' show kLongPressTimeout;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

// A6 (US-A6, freeze §5): edit and delete a message of your own in a ticket's
// discussion, behind `message_edits`. The server says per message whether it is
// `editable`; the phone shows what the server says and nothing more.

const _withEdits = LinkState(
  status: LinkStatus.online,
  capabilities: ['read', 'chat', 'task_control', 'approve'],
  features: [ticketDiscussionFeature, 'message_edits'],
  boardMembersExist: true,
  user: testUser,
);

const _withoutEdits = LinkState(
  status: LinkStatus.online,
  capabilities: ['read', 'chat', 'task_control', 'approve'],
  features: [ticketDiscussionFeature],
  boardMembersExist: true,
  user: testUser,
);

/// A post of a board member in the discussion, with what A6 adds to a message.
MessageView postOf(String id, String text, {UserView by = testUser, bool? editable, int? editedAtMs, int? deletedAtMs, List<MessageFileView> uploads = const [], List<FileRef> files = const []}) => MessageView(
      activity: const [],
      authorUser: by,
      createdAtMs: 0,
      deletedAtMs: deletedAtMs,
      editable: editable,
      editedAtMs: editedAtMs,
      files: files,
      id: id,
      ordinal: 0,
      queued: false,
      role: MessageRole.user,
      sources: const [],
      status: MessageStatus.complete,
      text: text,
      uploads: uploads,
    );

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  late MockRepository repo;

  Future<void> open(WidgetTester tester, List<MessageView> messages, {LinkState link = _withEdits, Stream<Event> chat = const Stream.empty()}) async {
    final ticket = TicketView.fromJson({
      ...ticketOf(column: ColumnKind.doing, ownerEmployeeId: 'forge', ownerName: 'Forge', rootTaskId: 'root').toJson(),
      'conversation_id': 'disc-1',
    });
    repo = boardRepository(detail: TicketDetail(asOfSeq: 1, ticket: ticket, tasks: const []), messages: snapshotOf('disc-1', messages: messages), link: link);
    when(() => repo.chatChanges).thenAnswer((_) => chat);
    when(() => repo.appends).thenAnswer((_) => const Stream.empty());
    when(() => repo.loadChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.load()).thenAnswer((_) async => loadOf());
    when(() => repo.boardMembers()).thenAnswer((_) async => const [testUser, bo]);
    await pumpBoardPage(tester, repo, TicketPage(ticketId: ticket.id, discussion: true), link: link);
  }

  Future<void> longPress(WidgetTester tester, String text) async {
    final gesture = await tester.startGesture(tester.getCenter(find.text(text)));
    await tester.pump(kLongPressTimeout + const Duration(milliseconds: 50));
    await gesture.up();
    await settleBoard(tester);
  }

  /// The writes so far; `writesOf` insists on at least one.
  List<Map<String, dynamic>> writes() {
    try {
      return writesOf(repo);
    } on TestFailure {
      return [];
    }
  }

  group('who may edit', () {
    testWidgets('the server says editable: the long press offers Edit and Delete', (tester) async {
      await open(tester, [postOf('m1', 'Keep the cost configurable.', editable: true)]);
      await longPress(tester, 'Keep the cost configurable.');
      expect(find.text('Edit'), findsOneWidget);
      expect(find.text('Delete'), findsOneWidget);
    });

    testWidgets('an edited message still offers Edit and Delete on a long press (also on its text, not only the bubble)', (tester) async {
      await open(tester, [postOf('m1', 'Keep the cost configurable.', editable: true, editedAtMs: 5)]);
      expect(find.text('edited'), findsOneWidget);
      await longPress(tester, 'Keep the cost configurable.');
      expect(find.text('Edit'), findsOneWidget);
      expect(find.text('Delete'), findsOneWidget);
    });

    testWidgets('after the user edits a message and the server confirms, the long press offers Edit and Delete again', (tester) async {
      final events = StreamController<Event>.broadcast();
      addTearDown(events.close);
      await open(tester, [postOf('m1', 'Keep the cost configurable.', editable: true)], chat: events.stream);
      await longPress(tester, 'Keep the cost configurable.');
      await tester.tap(find.text('Edit'));
      await settleBoard(tester);
      await tester.enterText(find.byKey(const ValueKey('message-edit-field')), 'Keep the cost configurable per tenant.');
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('message-edit-save')));
      await settleBoard(tester);
      when(() => repo.messages(any())).thenAnswer((_) async => snapshotOf('disc-1', messages: [postOf('m1', 'Keep the cost configurable per tenant.', editable: true, editedAtMs: 7)]));
      events.add(const Event(aggregateId: 'disc-1', atMs: 7, kind: EventKind.messageEdited, seq: 9));
      await settleBoard(tester);
      expect(find.text('edited'), findsOneWidget);
      await longPress(tester, 'Keep the cost configurable per tenant.');
      expect(find.text('Edit'), findsOneWidget);
      expect(find.text('Delete'), findsOneWidget);
    });

    testWidgets('choosing from the menu the way the E2E does, a second long press on the same message opens the menu again', (tester) async {
      await open(tester, [postOf('m1', 'Keep the cost configurable.', editable: true)]);
      await longPress(tester, 'Keep the cost configurable.');
      final copy = find.ancestor(of: find.text('Copy'), matching: find.byType(InkWell)).first;
      tester.widget<InkWell>(copy).onTap!();
      await settleBoard(tester);
      expect(find.text('Edit'), findsNothing, reason: 'the menu closed');
      await longPress(tester, 'Keep the cost configurable.');
      expect(find.text('Edit'), findsOneWidget);
    });

    testWidgets('not editable (someone elses, a decision feedback): neither item, not even disabled', (tester) async {
      await open(tester, [postOf('m1', 'Mine but fixed.', editable: false), postOf('m2', 'Bo wrote this.', by: bo, editable: false), postOf('m3', 'Older server.')]);
      await longPress(tester, 'Mine but fixed.');
      expect(find.text('Edit'), findsNothing);
      expect(find.text('Delete'), findsNothing);
      await longPress(tester, 'Older server.');
      expect(find.text('Edit'), findsNothing);
    });

    testWidgets('a server without message_edits hides both, even if a message claims editable', (tester) async {
      await open(tester, [postOf('m1', 'Keep the cost configurable.', editable: true, editedAtMs: 5)], link: _withoutEdits);
      await longPress(tester, 'Keep the cost configurable.');
      expect(find.text('Edit'), findsNothing);
      expect(find.text('Delete'), findsNothing);
      expect(find.text('edited'), findsNothing, reason: 'an older server shows no edited mark');
    });
  });

  group('edit', () {
    Future<void> startEdit(WidgetTester tester, String text) async {
      await longPress(tester, text);
      await tester.tap(find.text('Edit'));
      await settleBoard(tester);
    }

    testWidgets('the sheet holds the text and the hint of B3 word for word; Save writes message.edit with the new text', (tester) async {
      await open(tester, [postOf('m1', 'Keep the cost configurable.', editable: true)]);
      await startEdit(tester, 'Keep the cost configurable.');
      expect(find.text("Editing wakes no one. An employee that already read this keeps the original. This doesn't erase a secret: rotate it and put it in the vault."), findsOneWidget);
      expect(find.widgetWithText(TextField, 'Keep the cost configurable.'), findsOneWidget);
      await tester.enterText(find.byKey(const ValueKey('message-edit-field')), 'Keep the cost configurable per tenant.');
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('message-edit-save')));
      await settleBoard(tester);
      final edit = writes().singleWhere((w) => w['method'] == 'message.edit');
      expect(paramsOf(edit), {'message_id': 'm1', 'text': 'Keep the cost configurable per tenant.'});
      expect(find.byKey(const ValueKey('message-edit-field')), findsNothing, reason: 'the sheet closes');
    });

    testWidgets('nothing changed: Save closes without a write; empty: Save is off and says how to remove it', (tester) async {
      await open(tester, [postOf('m1', 'Keep the cost configurable.', editable: true)]);
      await startEdit(tester, 'Keep the cost configurable.');
      await tester.tap(find.byKey(const ValueKey('message-edit-save')));
      await settleBoard(tester);
      expect(writes().where((w) => w['method'] == 'message.edit'), isEmpty);

      await startEdit(tester, 'Keep the cost configurable.');
      await tester.enterText(find.byKey(const ValueKey('message-edit-field')), '  ');
      await tester.pump();
      expect(tester.widget<FilledButton>(find.byKey(const ValueKey('message-edit-save'))).onPressed, isNull);
      expect(find.text('To remove it, delete the message.'), findsOneWidget);
    });

    testWidgets('a new @name says it will not be told by an edit', (tester) async {
      await open(tester, [postOf('m1', 'Look at this @Forge.', editable: true)]);
      await startEdit(tester, 'Look at this @Forge.');
      await tester.enterText(find.byKey(const ValueKey('message-edit-field')), 'Look at this @Forge and @Bram.');
      await tester.pump();
      expect(find.text("@Bram won't be notified by an edit. Send a new message to reach them."), findsOneWidget);
    });

    testWidgets('a refused save keeps the sheet open with the reason', (tester) async {
      await open(tester, [postOf('m1', 'Keep the cost configurable.', editable: true)]);
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenThrow(const ProtocolException(ErrorCode.forbidden, 'This message can no longer be edited.'));
      await startEdit(tester, 'Keep the cost configurable.');
      await tester.enterText(find.byKey(const ValueKey('message-edit-field')), 'Something else');
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('message-edit-save')));
      await settleBoard(tester);
      expect(find.text('This message can no longer be edited.'), findsOneWidget);
      expect(find.byKey(const ValueKey('message-edit-field')), findsOneWidget);
    });

    test('newMentions: only names the old text did not have, in either case', () {
      expect(newMentions('hi @Forge', 'hi @forge and @Bram @Bram'), ['Bram']);
      expect(newMentions('', 'no names'), isEmpty);
    });
  });

  group('delete', () {
    testWidgets('the dialog is the text of B3 word for word; Delete writes message.delete, Cancel writes nothing', (tester) async {
      await open(tester, [postOf('m1', 'Oops, wrong channel.', editable: true)]);
      await longPress(tester, 'Oops, wrong channel.');
      await tester.tap(find.text('Delete'));
      await settleBoard(tester);
      expect(find.text('Delete this message?'), findsOneWidget);
      expect(find.text("A line stays that says it was deleted. The text stays in the audit log and backups, and employees that read it keep it. This doesn't erase a secret: rotate it and put it in the vault."), findsOneWidget);
      await tester.tap(find.text('Cancel'));
      await settleBoard(tester);
      expect(writes().where((w) => w['method'] == 'message.delete'), isEmpty);

      await longPress(tester, 'Oops, wrong channel.');
      await tester.tap(find.text('Delete'));
      await settleBoard(tester);
      await tester.tap(find.byKey(const ValueKey('message-delete-confirm')));
      await settleBoard(tester);
      expect(paramsOf(writes().singleWhere((w) => w['method'] == 'message.delete')), {'message_id': 'm1'});
    });

    testWidgets('a deleted message shows the placeholder, no text, no files, no copy, no menu', (tester) async {
      await open(tester, [
        postOf('m1', 'This message was deleted.', editable: false, deletedAtMs: 9, editedAtMs: 5, uploads: const [MessageFileView(byteLength: 3, name: 'secret-plan.pdf', path: 'x')], files: const [FileRef(id: 'f2', mediaType: 'image/png', name: 'shot.png', size: 1)]),
      ]);
      expect(find.text('This message was deleted.'), findsOneWidget);
      expect(find.byKey(const ValueKey('message-deleted-m1')), findsOneWidget);
      expect(find.textContaining('secret-plan'), findsNothing);
      expect(find.textContaining('shot.png'), findsNothing);
      expect(find.byKey(const Key('copy-user')), findsNothing);
      expect(find.text('edited'), findsNothing);
      await longPress(tester, 'This message was deleted.');
      expect(find.text('Copy'), findsNothing);
      expect(find.text('Edit'), findsNothing);
    });
  });

  group('edited', () {
    testWidgets('an edited message says so; a tap lists the earlier texts', (tester) async {
      await open(tester, [postOf('m1', 'Keep the cost configurable per tenant.', editedAtMs: 1762078440000)]);
      when(() => repo.messageEdits('m1')).thenAnswer((_) async => const MessageHistoryView(deleted: false, versions: [MessageVersionView(replacedAtMs: 1762078440000, text: 'Keep the cost configurable.')]));
      await tester.tap(find.byKey(const ValueKey('message-edited-m1')));
      await settleBoard(tester);
      expect(find.text('Earlier versions'), findsOneWidget);
      expect(find.text('Keep the cost configurable.'), findsOneWidget);
    });

    testWidgets('earlier versions that cannot be read: what failed and Try again', (tester) async {
      await open(tester, [postOf('m1', 'Now.', editedAtMs: 5)]);
      when(() => repo.messageEdits('m1')).thenAnswer((_) async => throw const ProtocolException(ErrorCode.internal, 'boom'));
      await tester.tap(find.byKey(const ValueKey('message-edited-m1')));
      await settleBoard(tester);
      expect(find.text('Could not load the earlier versions.'), findsOneWidget);
      expect(find.byKey(const ValueKey('message-versions-retry')), findsOneWidget);
    });
  });
}
