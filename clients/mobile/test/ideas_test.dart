import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/ideas/idea_list.dart';
import 'package:aurelius_mobile/features/ideas/ideas_bloc.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/builders.dart';

IdeaView idea(String id, String title, {String author = 'You', String? employeeId}) => ideaOf(id, title, author: author, employeeId: employeeId);

Widget list(List<IdeaView> ideas) => MaterialApp(
      theme: aureliusTheme(),
      home: Scaffold(body: IdeaList(ideas: ideas, onTap: (_) {}, onDrop: (_) {})),
    );

void main() {
  testWidgets('ideas are grouped into "From you" first and "From employees" below', (tester) async {
    await tester.pumpWidget(list([
      idea('1', 'Launch post', author: 'Iris', employeeId: 'mgr'),
      idea('2', 'Dark mode'),
      idea('3', 'Pricing page', author: 'Theo'),
      idea('4', 'Onboarding'),
    ]));

    expect(find.text('FROM YOU · 2'), findsOneWidget);
    expect(find.text('FROM EMPLOYEES · 2'), findsOneWidget);
    expect(tester.getTopLeft(find.byKey(const ValueKey('ideas-yours'))).dy, lessThan(tester.getTopLeft(find.byKey(const ValueKey('ideas-employees'))).dy));
    final managersTop = tester.getTopLeft(find.byKey(const ValueKey('ideas-employees'))).dy;
    for (final mine in ['Dark mode', 'Onboarding']) {
      expect(tester.getTopLeft(find.text(mine)).dy, lessThan(managersTop));
    }
    for (final theirs in ['Launch post', 'Pricing page']) {
      expect(tester.getTopLeft(find.text(theirs)).dy, greaterThan(managersTop));
    }

    expect(find.text('Idea from Iris'), findsOneWidget);
    expect(find.text('Idea from Theo'), findsOneWidget, reason: 'an older server only sends the author name');
    expect(find.text('You'), findsNWidgets(2));
    expect(find.byKey(const ValueKey('idea-accent-1')), findsOneWidget);
    expect(find.byKey(const ValueKey('idea-accent-3')), findsOneWidget);
    expect(find.byKey(const ValueKey('idea-accent-2')), findsNothing, reason: "the user's own ideas have no accent bar");
  });

  testWidgets('an empty section is left out', (tester) async {
    await tester.pumpWidget(list([idea('1', 'Launch post', author: 'Iris', employeeId: 'mgr')]));

    expect(find.byKey(const ValueKey('ideas-yours')), findsNothing);
    expect(find.text('FROM EMPLOYEES · 1'), findsOneWidget);
  });

  testWidgets('kind shows a badge and filters', (tester) async {
    expect(const IdeasState(ideas: [], kind: IdeaKind.bug).visible, isEmpty);
    expect(IdeasState(ideas: [ideaOf('1', 'A', kind: IdeaKind.bug), ideaOf('2', 'B')], kind: IdeaKind.bug).visible.single.id, '1');

    await tester.pumpWidget(list([ideaOf('x', 'T', kind: IdeaKind.bug), ideaOf('y', 'T')]));
    expect(find.byKey(const ValueKey('idea-kind-bug')), findsOneWidget);
    expect(find.byKey(const ValueKey('idea-kind-feature')), findsOneWidget);
  });

  testWidgets('the company-wide list labels each idea with its project and filters by project', (tester) async {
    final ideas = [ideaOf('1', 'Dark mode', projectId: 'a'), ideaOf('2', 'Export', projectId: 'b')];
    const names = {'a': 'Atlas', 'b': 'Borealis'};
    await tester.pumpWidget(MaterialApp(
      theme: aureliusTheme(),
      home: Scaffold(body: IdeaList(ideas: ideas, onTap: (_) {}, onDrop: (_) {}, projectName: (i) => names[i.projectId]!)),
    ));
    expect(find.text('Atlas'), findsOneWidget);
    expect(find.text('Borealis'), findsOneWidget);

    expect(IdeasState(ideas: ideas).visible.map((i) => i.id), ['1', '2'], reason: 'all projects by default');
    final onlyB = IdeasState(ideas: ideas, project: 'b');
    expect(onlyB.visible.single.id, '2');
    expect(onlyB.open, 1);
    expect(onlyB.copyWith(clearProject: true).visible, hasLength(2));
  });
}
