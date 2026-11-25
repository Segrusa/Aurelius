import 'package:aurelius_mobile/data/commands.dart';
import 'package:flutter_test/flutter_test.dart';

Map<String, dynamic> params(Map<String, dynamic> request) => request['params'] as Map<String, dynamic>;

void main() {
  test('a task write carries files and run', () {
    final create = Commands.createTask('p', 't', 'g', const [], files: ['/in/a'], run: {'effort': 'high'});
    expect(create['method'], 'task.create');
    expect(params(create)['files'], ['/in/a']);
    expect(params(create)['run'], {'effort': 'high'});
    expect(params(Commands.createTask('p', 't', 'g', const []))['files'], isEmpty);
  });

  test('a task has no repeat of its own any more', () {
    final create = Commands.createTask('p', 't', 'g', const []);
    expect(params(create).containsKey('repeat_every_minutes'), isFalse);
  });
}
