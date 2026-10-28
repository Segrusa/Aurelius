import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/markdown_paths.dart';
import 'package:aurelius_mobile/features/artifacts/artifacts.dart';
import 'package:aurelius_mobile/features/markdown/links.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('links in plain text', () {
    List<String> targets(String text) => [for (final l in plainTextLinks(text)) l.target];
    List<String> labels(String text) => [for (final l in plainTextLinks(text)) text.substring(l.start, l.end)];

    test('finds URLs without trailing punctuation', () {
      expect(labels('See https://x.io/a?b=1, then (http://y.io).'), ['https://x.io/a?b=1', 'http://y.io']);
      expect(labels('Wiki https://en.wikipedia.org/wiki/Foo_(bar)!'), ['https://en.wikipedia.org/wiki/Foo_(bar)']);
      expect(targets('go to www.example.com.'), ['https://www.example.com']);
    });

    test('finds .md paths next to URLs, on every line', () {
      const text = 'Read docs/a.md and https://x.io/b.md\nthen `c.md`';
      expect(labels(text), ['docs/a.md', 'https://x.io/b.md', 'c.md']);
      expect(targets(r'C:\w\x.md'), ['C:/w/x.md']);
    });

    test('leaves URLs in code alone', () {
      expect(plainTextLinks('run `curl https://x.io` now'), isEmpty);
      expect(plainTextLinks('```\nhttps://x.io\n```'), isEmpty);
      expect(labels('```\ncode\n```\nhttps://y.io'), ['https://y.io']);
    });
  });

  group('tapping and long-pressing links', () {
    late List<Uri> launched;
    String? copied;

    setUp(() {
      launched = [];
      copied = null;
      launchExternal = (uri) async {
        launched.add(uri);
        return true;
      };
    });

    Future<void> pump(WidgetTester tester, Widget child) async {
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
        if (call.method == 'Clipboard.setData') copied = (call.arguments as Map)['text'] as String;
        return null;
      });
      addTearDown(() => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, null));
      await tester.pumpWidget(MaterialApp(theme: aureliusTheme(), home: Scaffold(body: child)));
    }

    Offset centreOf(WidgetTester tester, String substring) {
      final found = find.textRange.ofSubstring(substring).evaluate().first;
      final paragraph = found.renderObject;
      final box = paragraph.getBoxesForSelection(TextSelection(baseOffset: found.textRange.start, extentOffset: found.textRange.end)).first;
      return paragraph.localToGlobal(box.toRect().center);
    }

    testWidgets('markdown links, autolinks and bare URLs open in the browser', (tester) async {
      await pump(tester, const MessageMarkdown(data: 'A [site](https://a.io), <https://b.io> and https://c.io.', scope: null));
      await tester.tapOnText(find.textRange.ofSubstring('site'));
      await tester.tapOnText(find.textRange.ofSubstring('https://b.io'));
      await tester.tapOnText(find.textRange.ofSubstring('https://c.io'));
      expect(launched.map((u) => '$u'), ['https://a.io', 'https://b.io', 'https://c.io']);
    });

    testWidgets('a URL in code is not a link', (tester) async {
      await pump(tester, const MessageMarkdown(data: 'Run `curl https://a.io` now', scope: null));
      await tester.tapAt(centreOf(tester, 'https://a.io'));
      await tester.pump();
      expect(launched, isEmpty);
    });

    testWidgets('long-press copies a link instead of opening it', (tester) async {
      await pump(tester, const MessageMarkdown(data: 'See https://a.io and docs/plan.md', scope: null));
      await tester.longPressAt(centreOf(tester, 'https://a.io'));
      await tester.pumpAndSettle();
      expect(copied, 'https://a.io');
      expect(launched, isEmpty);
      expect(find.text('Link copied.'), findsOneWidget);
      await tester.longPressAt(centreOf(tester, 'docs/plan.md'));
      await tester.pumpAndSettle();
      expect(copied, 'docs/plan.md');
    });

    testWidgets("links in the user's plain text open and copy too", (tester) async {
      await pump(tester, const LinkedText('Look at https://a.io please, not `https://b.io`', scope: null));
      await tester.tapOnText(find.textRange.ofSubstring('https://a.io'));
      expect(launched.map((u) => '$u'), ['https://a.io']);
      await tester.tapAt(centreOf(tester, 'https://b.io'));
      await tester.longPressAt(centreOf(tester, 'https://a.io'));
      await tester.pumpAndSettle();
      expect(launched, hasLength(1));
      expect(copied, 'https://a.io');
    });
  });
}
