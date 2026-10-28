import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/commands.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/ideas/idea_list.dart';
import 'package:aurelius_mobile/features/ideas/idea_page.dart';
import 'package:aurelius_mobile/features/ideas/idea_sheet.dart';
import 'package:aurelius_mobile/features/ideas/ideas_bloc.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';

class MockRepository extends Mock implements AureliusRepository {}

const _photo = MessageFileView(id: 'i:1:0', name: 'sketch.png', path: '.aurelius/uploads/sketch.png', byteLength: 2048);
const _doc = MessageFileView(id: 'i:1:1', name: 'spec.pdf', path: '.aurelius/uploads/spec.pdf', byteLength: 204800);

MockRepository _repository({List<String> features = const ['idea_attachments']}) {
  final repo = MockRepository();
  when(() => repo.online).thenAnswer((_) => const Stream.empty());
  when(() => repo.companyChanges).thenAnswer((_) => const Stream.empty());
  when(() => repo.linkState).thenReturn(LinkState(status: LinkStatus.connecting, capabilities: const ['read'], features: features));
  // No `upload.get` here: attachments stay chips without a thumbnail fetch.
  when(() => repo.uploadsFetchable).thenReturn(false);
  return repo;
}

void main() {
  test('idea commands carry files, removals and delete', () {
    expect((Commands.addIdea('p1', 'T', 'a\nb', files: ['/inbox/x.png'])['params'] as Map)['files'], ['/inbox/x.png']);
    expect((Commands.addIdea('p1', 'T', 'a\nb')['params'] as Map)['note'], 'a\nb', reason: 'line breaks go to the server as typed');
    final update = Commands.updateIdea('i1', 'T', null, addFiles: ['/inbox/y.pdf'], removeFiles: [_photo.path])['params'] as Map;
    expect(update['add_files'], ['/inbox/y.pdf']);
    expect(update['remove_files'], [_photo.path]);
    expect(Commands.deleteIdea('i1'), {'method': 'idea.delete', 'params': {'idea_id': 'i1'}});
    expect(EventKind.ideaDeleted.wire, startsWith('idea.'), reason: 'the ideas list refreshes on idea.* events');
  });

  test('attachments and Delete follow the idea_attachments feature', () {
    final on = IdeasBloc(_repository(), 'p1');
    final off = IdeasBloc(_repository(features: const []), 'p1');
    expect(on.attachmentsEnabled, isTrue);
    expect(off.attachmentsEnabled, isFalse);
    on.close();
    off.close();
  });

  Future<List<IdeaEdit?>> openPage(WidgetTester tester, {IdeaView? idea, required bool attachments}) async {
    final results = <IdeaEdit?>[];
    await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
      value: _repository(),
      child: MaterialApp(
        theme: aureliusTheme(),
        home: Builder(
          builder: (context) => Scaffold(
            body: TextButton(onPressed: () async => results.add(await IdeaPage.open(context, idea: idea, attachments: attachments)), child: const Text('open')),
          ),
        ),
      ),
    ));
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
    return results;
  }

  testWidgets('the note is a multi-line field and keeps its line breaks', (tester) async {
    final results = await openPage(tester, attachments: false);
    final note = tester.widget<TextField>(find.byKey(const Key('idea-note')));
    expect(note.keyboardType, TextInputType.multiline);
    expect(note.maxLines, greaterThan(1));
    expect(find.byKey(const Key('idea-attach')), findsNothing, reason: 'an older server keeps no files with ideas');

    await tester.enterText(find.byKey(const Key('idea-title')), 'Dark mode');
    await tester.enterText(find.byKey(const Key('idea-note')), 'First line\nSecond line');
    await tester.tap(find.byKey(const Key('idea-save')));
    await tester.pumpAndSettle();
    expect(results.single!.note, 'First line\nSecond line');
    expect(results.single!.files, isEmpty);
  });

  testWidgets('editing shows the attachments and returns the removed ones', (tester) async {
    final results = await openPage(tester, idea: ideaOf('1', 'Dark mode', attachments: const [_photo, _doc]), attachments: true);
    expect(find.byKey(const Key('idea-attach')), findsOneWidget);
    expect(find.textContaining('sketch.png'), findsOneWidget);
    expect(find.textContaining('spec.pdf'), findsOneWidget);

    final photo = find.byKey(Key('idea-attachment-${_photo.path}'));
    await tester.tap(find.descendant(of: photo, matching: find.byTooltip('Delete')));
    await tester.pump();
    expect(find.textContaining('sketch.png'), findsNothing);

    await tester.tap(find.byKey(const Key('idea-save')));
    await tester.pumpAndSettle();
    expect(results.single!.removeFiles, [_photo.path]);
    expect(results.single!.files, isEmpty);
  });

  Future<(IdeasBloc, MockRepository, List<String?>)> openSheet(WidgetTester tester, IdeaView idea, {required bool canDelete}) async {
    final repo = _repository();
    when(() => repo.ideas('p1')).thenAnswer((_) async => [idea]);
    final bloc = IdeasBloc(repo, 'p1')..add(const IdeasRefreshed());
    await tester.pump(const Duration(milliseconds: 300));
    final picked = <String?>[];
    await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
      value: repo,
      child: MaterialApp(
        theme: aureliusTheme(),
        home: Builder(
          builder: (context) => Scaffold(
            body: TextButton(
              onPressed: () async => picked.add(await showModalBottomSheet<String>(
                context: context,
                isScrollControlled: true,
                builder: (_) => BlocProvider.value(value: bloc, child: IdeaSheet(idea: idea, employees: const [], canDelete: canDelete)),
              )),
              child: const Text('open'),
            ),
          ),
        ),
      ),
    ));
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
    return (bloc, repo, picked);
  }

  testWidgets('the sheet shows the whole note with its line breaks and the files', (tester) async {
    final (bloc, _, _) = await openSheet(tester, ideaOf('1', 'Dark mode', note: 'One\nTwo\n\nThree', attachments: const [_photo]), canDelete: true);
    expect(tester.widget<SelectableText>(find.byKey(const Key('idea-sheet-note'))).data, 'One\nTwo\n\nThree');
    expect(find.textContaining('sketch.png'), findsOneWidget);
    bloc.close();
  });

  testWidgets('Delete is offered only on servers with idea_attachments', (tester) async {
    final (without, _, _) = await openSheet(tester, ideaOf('1', 'Dark mode'), canDelete: false);
    expect(find.byKey(const Key('idea-delete')), findsNothing);
    without.close();
  });

  testWidgets('Delete picks delete', (tester) async {
    final (bloc, _, picked) = await openSheet(tester, ideaOf('1', 'Dark mode'), canDelete: true);
    await tester.scrollUntilVisible(find.byKey(const Key('idea-delete')), 50);
    await tester.tap(find.byKey(const Key('idea-delete')));
    await tester.pumpAndSettle();
    expect(picked.single, 'delete');
    bloc.close();
  });

  testWidgets('a deleted idea closes its sheet', (tester) async {
    final (bloc, repo, picked) = await openSheet(tester, ideaOf('1', 'Dark mode'), canDelete: true);
    expect(find.byKey(const Key('idea-sheet-title')), findsOneWidget);
    when(() => repo.ideas('p1')).thenAnswer((_) async => []);
    bloc.add(const IdeasRefreshed());
    await tester.pump(const Duration(milliseconds: 300));
    await tester.pumpAndSettle();
    expect(find.byKey(const Key('idea-sheet-title')), findsNothing);
    expect(picked.single, isNull);
    bloc.close();
  });

  testWidgets('the list row counts the files', (tester) async {
    await tester.pumpWidget(MaterialApp(
      theme: aureliusTheme(),
      home: Scaffold(body: IdeaList(ideas: [ideaOf('1', 'Dark mode', attachments: const [_photo, _doc])], onTap: (_) {}, onDrop: (_) {})),
    ));
    expect(find.textContaining('2 files'), findsOneWidget);
  });
}
