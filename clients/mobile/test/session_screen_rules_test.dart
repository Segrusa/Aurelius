import 'package:aurelius_mobile/data/protocol/protocol.dart';
import 'package:aurelius_mobile/features/sessions/session_screen_rules.dart';
import 'package:flutter_test/flutter_test.dart';

ActivityView step(String tool, Map<String, Object?> input, {bool? ok = true}) => ActivityView(callId: '$tool-${input.hashCode}', input: input, offset: 0, ok: ok, tool: tool);

MessageView answer(String id, {List<ActivityView> activity = const [], String? runId}) => MessageView(
      activity: activity,
      createdAtMs: 0,
      files: const [],
      id: id,
      ordinal: 0,
      queued: false,
      role: MessageRole.assistant,
      runId: runId,
      sources: const [],
      status: MessageStatus.complete,
      text: 'text',
      uploads: const [],
    );

void main() {
  test('the footer names who answered and the model in words, not the connection', () {
    expect(answerFooter(author: 'Orbit', producedBy: 'Claude Code · claude-opus-5-5'), 'Orbit · Opus 5.5');
    expect(answerFooter(author: ' ', producedBy: 'claude-opus-5-5'), 'Opus 5.5');
    expect(answerFooter(author: 'Orbit'), 'Orbit');
    expect(answerFooter(), '');
  });

  test('files written counts each written file once and skips reads and failed writes', () {
    final messages = [
      answer('a', activity: [
        step('Read', {'file_path': '/r/a.dart'}),
        step('Edit', {'file_path': '/r/b.dart'}),
        step('Write', {'file_path': '/r/c.dart'}, ok: false),
      ]),
      answer('b', activity: [
        step('Edit', {'file_path': '/r/b.dart', 'old_string': 'x'}),
        step('MultiEdit', {'file_path': '/r/d.dart'}),
        step('NotebookEdit', {'notebook_path': '/r/e.ipynb'}),
      ]),
    ];
    expect(filesWritten(messages), ['/r/b.dart', '/r/d.dart', '/r/e.ipynb']);
  });

  test('the guidelines come from the latest message that has a run', () {
    expect(lastRunOf([answer('a', runId: 'r1'), answer('b', runId: 'r2'), answer('c')]), 'r2');
    expect(lastRunOf([answer('a')]), isNull);
  });

  test('an edit shows its old lines removed and its new lines added', () {
    final diff = editDiff(step('Edit', {'old_string': 'a\nb', 'new_string': 'c'}));
    expect(diff, const [DiffLine(false, 'a'), DiffLine(false, 'b'), DiffLine(true, 'c')]);
  });

  test('a multi-edit lists each edit in turn and a write only adds', () {
    final multi = editDiff(step('MultiEdit', {
      'edits': [
        {'old_string': 'a', 'new_string': 'b'},
        {'old_string': 'c', 'new_string': 'd'},
      ],
    }));
    expect(multi, const [DiffLine(false, 'a'), DiffLine(true, 'b'), DiffLine(false, 'c'), DiffLine(true, 'd')]);
    expect(editDiff(step('Write', {'content': 'x\ny'})), const [DiffLine(true, 'x'), DiffLine(true, 'y')]);
    expect(editDiff(step('Bash', {'command': 'ls'})), isEmpty);
  });

  test('the diff card is titled with the last two parts of the path', () {
    expect(diffTitle('/home/x/clients/mobile/lib/core/widgets/ios.dart'), 'widgets/ios.dart');
    expect(diffTitle(r'C:\repo\main.dart'), 'repo/main.dart');
    expect(diffTitle('ios.dart'), 'ios.dart');
  });

  test('the start reads day-month-year with the time', () {
    expect(startedLabel(DateTime(2026, 11, 20, 13, 32).millisecondsSinceEpoch), '20-11-2026, 13:32');
  });

  test('a session works only while somebody runs it', () {
    expect(sessionTone(true, null), StatusTone.working);
    expect(sessionTone(false, null), StatusTone.waiting);
    expect(sessionTone(true, SessionDispatch.choosing), StatusTone.waiting);
  });
}
