import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/chat/answer_body.dart';
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

class MockRepository extends Mock implements AureliusRepository {}

const withPreview = LinkState(status: LinkStatus.online, capabilities: ['read'], features: ['activity_preview']);
const older = LinkState(status: LinkStatus.online, capabilities: ['read']);

final preview = 'x' * 2048;
final whole = '${'x' * 2048}${'y' * 5939}'; // 7987 bytes: "7.8 KB"

ActivityView bash({String? output, bool? truncated, int? outputBytes, String command = 'ls'}) => ActivityView(
      callId: 'c1',
      input: {'command': command},
      offset: 0,
      ok: true,
      output: output,
      tool: 'Bash',
      truncated: truncated,
      outputBytes: outputBytes,
    );

void main() {
  late MockRepository repo;
  String? copied;

  setUp(() {
    repo = MockRepository();
    when(() => repo.link).thenAnswer((_) => const Stream.empty());
    copied = null;
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
      if (call.method == 'Clipboard.setData') copied = (call.arguments as Map)['text'] as String?;
      return null;
    });
  });

  Future<void> show(WidgetTester tester, ActivityView activity, {LinkState link = withPreview, String? messageId = 'm1'}) async {
    when(() => repo.linkState).thenReturn(link);
    await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
      value: repo,
      child: BlocProvider(
        create: (_) => ConnectionBloc(repo),
        child: MaterialApp(theme: aureliusTheme(), home: Scaffold(body: SingleChildScrollView(child: ActivityCard(activity: activity, messageId: messageId)))),
      ),
    ));
    await tester.tap(find.text('ls'));
    await tester.pump();
  }

  test('byte sizes in words', () {
    expect(byteSize(812), '812 B');
    expect(byteSize(2048), '2 KB');
    expect(byteSize(7987), '7.8 KB');
    expect(byteSize(20480), '20 KB');
    expect(byteSize(1258291), '1.2 MB');
  });

  testWidgets('a shortened output is marked; Show full output fetches it whole once', (tester) async {
    when(() => repo.activity('m1', 'c1')).thenAnswer((_) async => bash(output: whole));
    await show(tester, bash(output: preview, truncated: true, outputBytes: 7987));
    expect(find.text('Shortened: showing 2 KB of 7.8 KB'), findsOneWidget);

    await tester.tap(find.byKey(const Key('show-full-output')));
    await tester.pumpAndSettle();
    expect(find.byKey(const Key('activity-shortened')), findsNothing);
    expect(find.text(whole), findsOneWidget);
    verify(() => repo.activity('m1', 'c1')).called(1);

    // Copy now copies what is shown, without asking again.
    await tester.tap(find.byKey(const Key('copy-output')));
    await tester.pumpAndSettle();
    expect(copied, whole);
    verifyNever(() => repo.activity(any(), any()));
  });

  testWidgets('copying a shortened output fetches and copies the whole output', (tester) async {
    when(() => repo.activity('m1', 'c1')).thenAnswer((_) async => bash(output: whole));
    await show(tester, bash(output: preview, truncated: true, outputBytes: 7987));
    await tester.tap(find.byKey(const Key('copy-output')));
    await tester.pumpAndSettle();
    expect(copied, whole);
  });

  testWidgets('a long command that was cut can be shown in full', (tester) async {
    final long = 'echo ${'a' * 1100}';
    when(() => repo.activity('m1', 'c1')).thenAnswer((_) async => bash(output: 'ok', command: long));
    when(() => repo.linkState).thenReturn(withPreview);
    await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
      value: repo,
      child: BlocProvider(
        create: (_) => ConnectionBloc(repo),
        child: MaterialApp(
            theme: aureliusTheme(),
            home: Scaffold(body: SingleChildScrollView(child: ActivityCard(activity: bash(output: 'ok', command: long.substring(0, 1024), truncated: true), messageId: 'm1')))),
      ),
    ));
    await tester.tap(find.byType(InkWell).first);
    await tester.pump();
    expect(find.text('Shortened'), findsOneWidget);
    await tester.tap(find.text('Show in full'));
    await tester.pumpAndSettle();
    expect(find.byWidgetPredicate((w) => w is SelectableText && w.data == long), findsOneWidget, reason: 'the whole command, under its step');
  });

  testWidgets('a call no longer kept says so and keeps the preview', (tester) async {
    when(() => repo.activity('m1', 'c1')).thenThrow(const ProtocolException(ErrorCode.notFound, 'This tool call is no longer kept.'));
    await show(tester, bash(output: preview, truncated: true, outputBytes: 7987));
    await tester.tap(find.byKey(const Key('show-full-output')));
    await tester.pumpAndSettle();
    expect(find.text('This tool call is no longer kept on the PC.'), findsOneWidget);
    expect(find.text(preview), findsOneWidget);
    expect(find.byKey(const Key('show-full-output')), findsOneWidget);
  });

  testWidgets('a server without activity_preview shows no mark or action', (tester) async {
    await show(tester, bash(output: preview, truncated: true, outputBytes: 7987), link: older);
    expect(find.byKey(const Key('activity-shortened')), findsNothing);
    expect(find.byKey(const Key('show-full-output')), findsNothing);
    await tester.tap(find.byKey(const Key('copy-output')));
    await tester.pumpAndSettle();
    expect(copied, preview);
    verifyNever(() => repo.activity(any(), any()));
  });

  testWidgets('a whole call shows no mark', (tester) async {
    await show(tester, bash(output: 'done'));
    expect(find.text('done'), findsOneWidget);
    expect(find.byKey(const Key('activity-shortened')), findsNothing);
  });

  test('frame_too_large reads as an error with words', () {
    expect(const ProtocolException(ErrorCode.frameTooLarge, '').shown, "The PC's reply is too large to send.");
    expect(const ProtocolException(ErrorCode.frameTooLarge, 'Too large (5 MB).').toString(), 'Too large (5 MB).');
  });
}
