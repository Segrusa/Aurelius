import 'package:aurelius_mobile/core/clipboard/image_clipboard.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/gateway_api.dart' show UploadedFile;
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/ticket_page.dart';
import 'package:aurelius_mobile/features/board/ticket_text_dialog.dart';

import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/tickets.dart';

class MockRepository extends Mock implements AureliusRepository {}

void main() {
  setUpAll(() => registerFallbackValue(File('')));

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
    Future<List<TicketTextEdit?>> open(WidgetTester tester, {bool required = true, int room = 0, AureliusRepository? repository}) async {
      final answers = <TicketTextEdit?>[];
      await tester.pumpWidget(
        RepositoryProvider<AureliusRepository>.value(
          value: repository ?? MockRepository(),
          child: MaterialApp(
            home: Scaffold(
              body: Builder(
                builder: (context) => TextButton(
                  onPressed: () async => answers.add(await editTicketText(context, label: 'Title', initial: 'Old', required: required, room: room)),
                  child: const Text('open'),
                ),
              ),
            ),
          ),
        ),
      );
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

    /// A repository whose upload keeps the name and makes the path unique, as the gateway does.
    MockRepository gateway() {
      final repository = MockRepository();
      var n = 0;
      when(() => repository.upload(any(), any())).thenAnswer((call) async => UploadedFile(path: '/inbox/${++n}/${call.positionalArguments[1]}', name: call.positionalArguments[1] as String, byteLength: 4));
      return repository;
    }

    Future<void> insertImage(WidgetTester tester) async {
      final field = tester.widget<TextField>(find.byKey(const ValueKey('ticket-text-field')));
      // The paste writes a real temporary file, so it runs outside the fake clock.
      await tester.runAsync(() async {
        field.contentInsertionConfiguration!.onContentInserted(KeyboardInsertedContent(mimeType: 'image/png', uri: 'content://x', data: Uint8List.fromList([137, 80, 78, 71])));
        await Future<void>.delayed(const Duration(milliseconds: 200));
      });
      await tester.pumpAndSettle();
    }

    testWidgets('two pasted images both show, × removes one, and Save sends the other', (tester) async {
      final answers = await open(tester, room: 8, repository: gateway());
      await insertImage(tester);
      await insertImage(tester);
      expect(tester.takeException(), isNull);
      expect(find.text('Pasted image.png'), findsOneWidget);
      expect(find.text('Pasted image 2.png'), findsOneWidget);
      await tester.tap(find.descendant(of: find.byKey(const ValueKey('ticket-text-file-/inbox/1/Pasted image.png')), matching: find.byIcon(Icons.close)));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('ticket-text-save')));
      await tester.pumpAndSettle();
      expect([for (final a in answers) (a?.text, a?.files.join(','))], [('Old', '/inbox/2/Pasted image 2.png')]);
    });

    testWidgets('a failed upload says so and adds nothing', (tester) async {
      final repository = MockRepository();
      when(() => repository.upload(any(), any())).thenThrow(Exception('offline'));
      await open(tester, room: 8, repository: repository);
      await insertImage(tester);
      expect(find.textContaining('offline'), findsOneWidget);
      expect(find.byIcon(Icons.image_outlined), findsNothing);
    });

    testWidgets('with the ticket full no image is uploaded', (tester) async {
      final repository = gateway();
      await open(tester, room: 1, repository: repository);
      await insertImage(tester);
      await insertImage(tester);
      expect(find.text('This ticket takes no more files.'), findsOneWidget);
      verify(() => repository.upload(any(), any())).called(1);
    });

    testWidgets('Cancel gives nothing back', (tester) async {
      final answers = await open(tester, room: 8);
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      expect(answers, [null]);
    });
  });

  group('the update a title or description edit sends', () {
    final ticket = ticketOf(title: 'Old');

    test('only pasted files: the text stays out and the files go', () {
      final request = ticketTextUpdate(ticket, (text: 'Old', files: ['/inbox/a.png']), title: true)!;
      expect(request['params'], {
        'ticket_id': ticket.id,
        'add_files': ['/inbox/a.png'],
      });
    });

    test('a changed text without files sends no add_files, so an older server takes it', () {
      final request = ticketTextUpdate(ticket, (text: 'New', files: const []), title: true)!;
      expect(request['params'], {'ticket_id': ticket.id, 'title': 'New'});
    });

    test('nothing changed, or an empty title, sends nothing', () {
      expect(ticketTextUpdate(ticket, (text: 'Old', files: const []), title: true), isNull);
      expect(ticketTextUpdate(ticket, (text: '', files: ['/inbox/a.png']), title: true), isNull);
    });

    test('an emptied description clears it', () {
      final described = TicketView.fromJson({...ticket.toJson(), 'description': 'Steps'});
      expect(ticketTextUpdate(described, (text: '', files: const []), title: false)?['params'], {'ticket_id': ticket.id, 'description': ''});
    });
  });

  test('repeated pasted images get a number', () {
    expect(pastedImageName([], 'png'), 'Pasted image.png');
    expect(pastedImageName(['Pasted image.png', 'Pasted image 2.png'], 'png'), 'Pasted image 3.png');
    expect(pastedImageName(['Pasted image.png'], 'jpg'), 'Pasted image.jpg');
  });
}
