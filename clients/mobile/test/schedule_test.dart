import 'package:aurelius_mobile/data/commands.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:flutter_test/flutter_test.dart';

Map<String, dynamic> params(Map<String, dynamic> request) => request['params'] as Map<String, dynamic>;

void main() {
  test('recurrences read and write the wire shape and read aloud', () {
    final weekly = Recurrence.fromJson({'kind': 'weekly', 'weekdays': [0, 4], 'at_minute': 540});
    expect(weekly.toJson(), {'kind': 'weekly', 'weekdays': [0, 4], 'at_minute': 540});
    expect(weekly.label, 'Mon, Fri at 09:00');
    expect(Recurrence.fromJson({'kind': 'daily', 'at_minute': 570}).label, 'Daily at 09:30');
    expect(Recurrence.fromJson({'kind': 'every_hours', 'hours': 6}).label, 'Every 6 hours');
    expect(const RecurrenceEveryHours(hours: 6).toJson(), {'kind': 'every_hours', 'hours': 6});
  });

  test('a schedule with attachments parses', () {
    final s = TaskScheduleView.fromJson({
      'id': 's1',
      'project_id': 'p',
      'employee_id': 'm',
      'employee_name': 'Iris',
      'title': 'Report',
      'goal': 'a b',
      'criteria': ['x'],
      'attachments': [
        {'name': 'a.png', 'path': '.aurelius/uploads/a.png', 'byte_length': 10}
      ],
      'run': {'mode': 'plan'},
      'recurrence': {'kind': 'daily', 'at_minute': 60},
      'paused': true,
      'created_at_ms': 1,
    });
    expect(s.attachments.single.name, 'a.png');
    expect(s.paused, isTrue);
    expect(s.nextRunMs, isNull);
    expect(s.run.choices['mode'], 'plan');
  });

  test('writes carry files, run and recurrence', () {
    final create = Commands.createTask('p', 't', 'g', const [], files: ['/in/a'], run: {'effort': 'high'});
    expect(create['method'], 'task.create');
    expect(params(create)['files'], ['/in/a']);
    expect(params(create)['run'], {'effort': 'high'});
    expect(params(Commands.createTask('p', 't', 'g', const []))['files'], isEmpty);
    final schedule = Commands.createTaskSchedule('p', 'm', 't', 'g', const [], const RecurrenceDaily(atMinute: 60), paused: true);
    expect(schedule['method'], 'recurring_task.create');
    expect(params(schedule)['recurrence'], {'kind': 'daily', 'at_minute': 60});
    expect(params(schedule)['paused'], true);
    expect(params(Commands.updateTaskSchedule('s', 'm', 't', 'g', const [], const RecurrenceDaily(atMinute: 60), removeFiles: ['x']))['remove_files'], ['x']);
  });

  test('a task has no repeat of its own any more', () {
    final create = Commands.createTask('p', 't', 'g', const []);
    expect(params(create).containsKey('repeat_every_minutes'), isFalse);
  });
}
