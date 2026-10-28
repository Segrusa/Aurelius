import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/core/theme/tokens.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/chat/answer_body.dart';
import 'package:aurelius_mobile/features/markdown/links.dart';
import 'package:aurelius_mobile/features/markdown/mermaid.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:markdown/markdown.dart' as md;

import 'support/builders.dart';

const flow = 'graph TD\n  A[Start] --> B{Ok?}\n  B -->|yes| C[Done]';
const message = 'Here is the flow:\n\n```mermaid\n$flow\n```\n\nThat is all.';

/// Stands in for the webview: records each request and how often a page was created.
class FakeDiagram extends StatefulWidget {
  const FakeDiagram({super.key, required this.request});

  final MermaidRequest request;

  static final requests = <MermaidRequest>[];
  static var pages = 0;

  @override
  State<FakeDiagram> createState() => _FakeDiagramState();
}

class _FakeDiagramState extends State<FakeDiagram> {
  @override
  void initState() {
    super.initState();
    FakeDiagram.pages++;
  }

  @override
  Widget build(BuildContext context) {
    FakeDiagram.requests.add(widget.request);
    return const ColoredBox(key: Key('fake-diagram'), color: Colors.transparent);
  }
}

List<md.Node> parse(String text) =>
    md.Document(blockSyntaxes: const [MermaidBlockSyntax()], encodeHtml: false).parseLines(text.split('\n'));

Widget app(Widget child) => MaterialApp(theme: aureliusTheme(), home: Scaffold(body: SingleChildScrollView(child: child)));

void main() {
  final original = mermaidView;
  late bool dark;

  setUp(() {
    dark = Tokens.dark;
    clearMermaidOutcomes();
    FakeDiagram.requests.clear();
    FakeDiagram.pages = 0;
    mermaidView = (request) => FakeDiagram(request: request);
  });

  tearDown(() {
    mermaidView = original;
    Tokens.dark = dark;
  });

  group('the mermaid fence', () {
    test('a closed ```mermaid fence becomes a diagram with its source', () {
      final node = parse('```mermaid\n$flow\n```').single as md.Element;
      expect(node.tag, 'mermaid');
      expect(node.attributes['source'], flow);
      expect((parse('~~~ Mermaid \nsequenceDiagram\n  A->>B: hi\n~~~~').single as md.Element).attributes['source'], 'sequenceDiagram\n  A->>B: hi');
    });

    test('an unclosed fence (still streaming) and other languages stay code', () {
      expect((parse('```mermaid\ngraph TD\n  A --> B').single as md.Element).tag, 'pre');
      expect((parse('````mermaid\ngraph TD\n```').single as md.Element).tag, 'pre');
      expect((parse('```mermaidx\ngraph TD\n```').single as md.Element).tag, 'pre');
      expect((parse('```python\nprint(1)\n```').single as md.Element).tag, 'pre');
    });

    test('works inside a quote', () {
      final quote = parse('> ```mermaid\n> graph LR; A-->B\n> ```').single as md.Element;
      expect((quote.children!.single as md.Element).tag, 'mermaid');
    });
  });

  test('the page answers with a height or an error', () {
    expect(parseMermaidMessage('ok:240'), const MermaidOutcome.drawn(240));
    expect(parseMermaidMessage('error')!.failed, isTrue);
    expect(parseMermaidMessage('ok:0')!.failed, isTrue);
    expect(parseMermaidMessage('ok:abc')!.failed, isTrue);
    expect(parseMermaidMessage('hello'), isNull);
  });

  test('the source goes into the page as a JSON string', () {
    const evil = "graph TD\n A[\"');alert(1);//\"] --> B";
    expect(mermaidRenderCall(evil, dark: true, maxHeight: 420.4), 'aureliusRender("graph TD\\n A[\\"\');alert(1);//\\"] --> B", true, 420, false);');
    expect(mermaidRenderCall('graph TD', dark: false, zoom: true), 'aureliusRender("graph TD", false, 0, true);');
  });

  testWidgets('a mermaid block shows its code until drawn, then the diagram', (tester) async {
    await tester.pumpWidget(app(const MessageMarkdown(data: message, scope: null)));
    expect(find.text('Here is the flow:'), findsOneWidget);
    expect(find.byKey(const Key('mermaid-code')), findsOneWidget);
    final request = FakeDiagram.requests.last;
    expect(request.source, flow);
    expect(request.dark, Tokens.dark);
    expect(request.maxHeight, mermaidInlineMaxHeight);
    expect(request.zoom, isFalse);

    request.onOutcome(const MermaidOutcome.drawn(180));
    await tester.pump();
    expect(find.byKey(const Key('mermaid-code')), findsNothing);
    expect(tester.getSize(find.byKey(const Key('fake-diagram'))).height, 180);
    // The page that drew it is kept, not loaded again.
    expect(FakeDiagram.pages, 1);
  });

  testWidgets('invalid Mermaid falls back to the code block', (tester) async {
    await tester.pumpWidget(app(const MessageMarkdown(data: '```mermaid\nnot a diagram\n```', scope: null)));
    FakeDiagram.requests.last.onOutcome(const MermaidOutcome.failed());
    await tester.pump();
    expect(find.byKey(const Key('mermaid-code')), findsOneWidget);
    expect(find.text('not a diagram'), findsOneWidget);
    expect(find.byKey(const Key('fake-diagram')), findsNothing);

    // Seen again (scrolled back), it goes straight to the code.
    await tester.pumpWidget(app(const SizedBox()));
    FakeDiagram.requests.clear();
    await tester.pumpWidget(app(const MessageMarkdown(data: '```mermaid\nnot a diagram\n```', scope: null)));
    expect(find.byKey(const Key('mermaid-code')), findsOneWidget);
    expect(FakeDiagram.requests, isEmpty);
  });

  testWidgets('a drawn diagram keeps its size when built again', (tester) async {
    await tester.pumpWidget(app(const MessageMarkdown(data: message, scope: null)));
    FakeDiagram.requests.last.onOutcome(const MermaidOutcome.drawn(150));
    await tester.pumpWidget(app(const SizedBox()));
    await tester.pumpWidget(app(const MessageMarkdown(data: message, scope: null)));
    expect(find.byKey(const Key('mermaid-code')), findsNothing);
    expect(tester.getSize(find.byKey(const Key('fake-diagram'))).height, 150);
  });

  testWidgets('diagrams follow the theme', (tester) async {
    Tokens.dark = false;
    await tester.pumpWidget(app(const MessageMarkdown(data: message, scope: null)));
    expect(FakeDiagram.requests.last.dark, isFalse);
    Tokens.dark = true;
    await tester.pumpWidget(MaterialApp(theme: aureliusTheme(), home: const Scaffold(body: SingleChildScrollView(child: MessageMarkdown(data: message, scope: null)))));
    expect(FakeDiagram.requests.last.dark, isTrue);
  });

  testWidgets('a streaming answer keeps the code block, a finished one draws it', (tester) async {
    await tester.pumpWidget(app(AnswerBody(message: messageOf('a1', MessageRole.assistant, message, status: MessageStatus.partial))));
    expect(find.textContaining('A[Start]'), findsOneWidget);
    expect(FakeDiagram.requests, isEmpty);

    await tester.pumpWidget(app(AnswerBody(message: messageOf('a1', MessageRole.assistant, message))));
    expect(FakeDiagram.requests, isNotEmpty);
    expect(FakeDiagram.requests.last.source, flow);
  });

  testWidgets('tapping a diagram opens it full screen with its source to copy', (tester) async {
    String? clipboard;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
      if (call.method == 'Clipboard.setData') clipboard = (call.arguments as Map)['text'] as String;
      return null;
    });
    addTearDown(() => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, null));

    await tester.pumpWidget(app(const MessageMarkdown(data: message, scope: null)));
    FakeDiagram.requests.last.onOutcome(const MermaidOutcome.drawn(120));
    await tester.pump();
    await tester.tap(find.byKey(const Key('mermaid-diagram')));
    await tester.pumpAndSettle();
    expect(find.byType(MermaidPage), findsOneWidget);
    expect(FakeDiagram.requests.last.zoom, isTrue);

    await tester.tap(find.byKey(const Key('mermaid-copy')));
    await tester.pump();
    expect(clipboard, flow);

    await tester.tap(find.byKey(const Key('mermaid-toggle-code')));
    await tester.pump();
    expect(find.descendant(of: find.byType(MermaidPage), matching: find.byKey(const Key('mermaid-code'))), findsOneWidget);
    await tester.pump(const Duration(seconds: 2));
  });
}
