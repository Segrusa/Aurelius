import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/ticket_page.dart';
import 'package:aurelius_mobile/features/board/ticket_text_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/tickets.dart';

void main() {
  group('pasting into an existing ticket (ticket #3)', () {
    const older = LinkState(status: LinkStatus.online, capabilities: ['read']);
    const newer = LinkState(status: LinkStatus.online, capabilities: ['read'], features: [ticketAddFilesFeature]);
    TicketView withFiles(int count) => TicketView.fromJson({
          ...ticketOf().toJson(),
          'attachments': [for (var i = 0; i < count; i++) const MessageFileView(byteLength: 1, name: 'a.png', path: 'a.png').toJson()],
        });

    test('a server without ticket_add_files takes no pasted files', () {
      expect(ticketFileRoom(withFiles(0), older), 0);
    });

    test('the room is what the ticket still takes, never below zero', () {
      expect(ticketFileRoom(withFiles(0), newer), 8);
      expect(ticketFileRoom(withFiles(3), newer), 5);
      expect(ticketFileRoom(withFiles(8), newer), 0);
    });

    /// Opens the dialog; the list gets its answer once it closes.
    Future<List<TicketTextEdit?>> open(WidgetTester tester, {bool required = true, int room = 0}) async {
      final answers = <TicketTextEdit?>[];
      await tester.pumpWidget(MaterialApp(
        home: Builder(
          builder: (context) => TextButton(
            onPressed: () async => answers.add(await editTicketText(context, label: 'Title', initial: 'Old', required: required, room: room)),
            child: const Text('open'),
          ),
        ),
      ));
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      return answers;
    }

    testWidgets('Save gives the text and no files when nothing was pasted', (tester) async {
      final answers = await open(tester, room: 8);
      await tester.enterText(find.byKey(const ValueKey('ticket-text-field')), '  New title ');
      await tester.tap(find.byKey(const ValueKey('ticket-text-save')));
      await tester.pumpAndSettle();
      expect([for (final a in answers) (a?.text, a?.files.length)], [('New title', 0)]);
    });

    testWidgets('a required field cannot be saved empty', (tester) async {
      await open(tester);
      await tester.enterText(find.byKey(const ValueKey('ticket-text-field')), '  ');
      await tester.pump();
      expect(tester.widget<FilledButton>(find.byKey(const ValueKey('ticket-text-save'))).onPressed, isNull);
    });

    testWidgets('without room the field keeps its own edit menu and takes no keyboard images', (tester) async {
      await open(tester);
      final field = tester.widget<TextField>(find.byKey(const ValueKey('ticket-text-field')));
      expect((field.contentInsertionConfiguration, field.contextMenuBuilder), (null, null));
    });

    testWidgets('with room the field takes a keyboard\'s image and offers Paste Image', (tester) async {
      await open(tester, room: 2);
      final field = tester.widget<TextField>(find.byKey(const ValueKey('ticket-text-field')));
      expect(field.contentInsertionConfiguration?.allowedMimeTypes, contains('image/png'));
      expect(field.contextMenuBuilder, isNotNull);
    });
  });
}
