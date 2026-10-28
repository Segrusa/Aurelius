import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/commands.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/chat/answer_body.dart';
import 'package:aurelius_mobile/features/chat/chat_bloc.dart';
import 'package:aurelius_mobile/features/chat/screenshot_page.dart';
import 'package:flutter/material.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';

class MockRepository extends Mock implements AureliusRepository {}

const online = LinkState(status: LinkStatus.online);

void main() {
  setUpAll(() {
    registerFallbackValue(<String, dynamic>{});
  });

  test('screen commands and approvals read as what they are', () {
    expect(Commands.setScreenAllowed('c', true), {
      'method': 'conversation.set_screen_allowed',
      'params': {'allowed': true, 'conversation_id': 'c'},
    });
    final shot = approvalOf('x', tool: 'claude.mcp__aurelius__screenshot', requester: 'Alex', arguments: {});
    expect(shot.isScreen, isTrue);
    expect(shot.title, 'Alex wants to see your screen');
    final windows = approvalOf('y', tool: 'claude.mcp__aurelius__list_windows', requester: 'Alex', arguments: {});
    expect(windows.title, 'Alex wants to see which windows are open');
  });

  group('ChatBloc screen', () {
    late MockRepository repo;

    setUp(() {
      repo = MockRepository();
      when(() => repo.online).thenAnswer((_) => const Stream.empty());
      when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.appends).thenAnswer((_) => const Stream.empty());
      when(() => repo.linkState).thenReturn(online);
      when(() => repo.messages('conv-1')).thenAnswer((_) async => snapshotOf('conv-1'));
    });

    blocTest<ChatBloc, ChatState>(
      'screenshots ask by default, the switch turns on with the command, and the PC can refuse',
      build: () => ChatBloc(repo, conversationId: 'conv-1', projectId: 'p'),
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 100));
        expect(bloc.state.screenAllowed, isFalse);
        when(() => repo.write(any(), retry: false)).thenAnswer((_) async => receiptOf(const CommandResultScreenAllowedSet(allowed: true, conversationId: 'conv-1')));
        bloc.add(const ChatScreenToggled());
        await Future<void>.delayed(const Duration(milliseconds: 100));
        expect(bloc.state.screenAllowed, isTrue);
        expect(verify(() => repo.write(captureAny(), retry: false)).captured.single, Commands.setScreenAllowed('conv-1', true));
        bloc.add(const ChatScreenToggled());
        await Future<void>.delayed(const Duration(milliseconds: 100));
        expect(bloc.state.screenAllowed, isFalse);
        when(() => repo.write(any(), retry: false)).thenThrow(const ProtocolException(ErrorCode.forbidden, 'Needs the Approve right.'));
        bloc.add(const ChatScreenToggled());
        await Future<void>.delayed(const Duration(milliseconds: 100));
      },
      wait: const Duration(milliseconds: 100),
      verify: (bloc) {
        expect(bloc.state.screenAllowed, isFalse, reason: 'a refused switch goes back');
        expect(bloc.state.notice, 'Needs the Approve right.');
      },
    );
  });

  test('the screenshot id is read from the tool output, never a path', () {
    const id = '0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b';
    expect(screenshotId('Screenshot of monitor 1 of 2, 2560x1440. Saved as screenshot $id.'), id);
    expect(screenshotId('Saved as screenshot ../../etc/passwd.'), isNull);
    expect(screenshotId(null), isNull);
  });

  testWidgets('a screenshot block shows View; other tools do not', (tester) async {
    const id = '0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b';
    ActivityView shot(String? output) => ActivityView(callId: 'c1', tool: 'mcp__aurelius__screenshot', input: const {'window': 'Excel'}, output: output, ok: true, offset: 0);
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: ActivityCard(activity: shot('Screenshot of window "Book1 - Excel", 1200x800. Saved as screenshot $id.')))));
    expect(find.text('Screenshot of window "Excel"'), findsOneWidget);
    expect(find.byKey(const Key('view-screenshot')), findsOneWidget);
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: ActivityCard(activity: shot('No open window matches "Excel".')))));
    expect(find.byKey(const Key('view-screenshot')), findsNothing, reason: 'nothing was kept: no button');
  });
}
