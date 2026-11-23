import 'package:aurelius_mobile/features/chat/chat_view.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('a queued message says when the machine holds it', () {
    expect(queuedLine('machine'), 'Waiting · the machine is busy, starts when there is room');
  });

  test('otherwise it waits for the current answer, also on a server that does not say', () {
    expect(queuedLine(null), 'Waiting · sends after the current answer');
  });
}
