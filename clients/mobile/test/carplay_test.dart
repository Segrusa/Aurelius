import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/carplay/carplay_selection.dart';
import 'package:aurelius_mobile/features/carplay/speech_text.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/builders.dart';

ProjectSummary project(String id, {ProjectStatus status = ProjectStatus.active, int createdAtMs = 1}) =>
    projectOf(id: id, name: 'Project $id', status: status, createdAtMs: createdAtMs);

EmployeeSummary employee(String id, {String? reportsTo = 'boss', List<int> activity = const [], LifecycleStatus status = LifecycleStatus.active}) => employeeOf(
      id: id,
      reportsTo: reportsTo,
      name: 'Employee $id',
      role: 'Lead',
      status: status,
      conversationId: 'c-$id',
      conversations: [for (final (i, at) in activity.indexed) conversationOf('c-$id-$i', updatedAtMs: at, messageCount: 1)],
    );

TaskView task(String id, {TaskStatus status = TaskStatus.waiting, String? waitingReason = 'input', int pendingApprovals = 0, int updatedAtMs = 1, String? employeeName = 'Max'}) => taskOf(
      id: id,
      employeeId: employeeName == null ? null : 'm',
      employeeName: employeeName,
      title: 'Task $id',
      goal: '',
      status: status,
      waitingReason: waitingReason,
      updatedAtMs: updatedAtMs,
      pendingApprovals: pendingApprovals,
      conversationId: 'tc-$id',
    );

void main() {
  group('speakable', () {
    test('drops markdown, keeps the words', () {
      const answer = '## Summary\n\n'
          'I fixed the **login** bug in `auth.dart`, see _the notes_.\n\n'
          '- Added a *retry*\n'
          '- Updated [the docs](https://example.com/docs)\n\n'
          '> Quoted ~~old~~ text';
      expect(speakable(answer), 'Summary. I fixed the login bug in auth.dart, see the notes. Added a retry. Updated the docs. Quoted old text');
    });

    test('leaves code blocks out and names links instead of reading them', () {
      const answer = 'Run this:\n\n```dart\nvoid main() {\n  print(1);\n}\n```\n\nDetails at https://example.com/a?b=c.';
      expect(speakable(answer), 'Run this: $codeLeftOut Details at a link');
    });

    test('an unclosed code block is left out to the end', () {
      expect(speakable('Here:\n```\nlet x = 1;\nlet y = 2;'), 'Here: $codeLeftOut');
    });

    test('long inline code is not spelled out, snake_case stays', () {
      expect(speakable('Set `my_flag` and run `cargo test -p aurelius-daemon --test delegation -- --nocapture`.'), 'Set my_flag and run code.');
    });

    test('tables are read row by row, rules and images vanish', () {
      const answer = '| Task | State |\n|---|:---:|\n| Login | done |\n\n---\n\n![chart](x.png)\nAll good.';
      expect(speakable(answer), 'Task, State. Login, done. All good.');
    });

    test('numbered items and headings become sentences; html is stripped', () {
      expect(speakable('# Plan\n1. Build it\n2) Ship it!\n<br/>Thanks'), 'Plan. Build it. Ship it! Thanks');
    });

    test('plain text passes unchanged', () {
      expect(speakable('Done. The branch is ready for review.'), 'Done. The branch is ready for review.');
    });
  });

  group('shortenForSpeech', () {
    test('short answers are read whole', () {
      expect(shortenForSpeech('One. Two.', limit: 100), 'One. Two.');
    });

    test('long answers keep whole first sentences and point to the phone', () {
      final sentence = 'This sentence is about fifty characters long, yes.';
      final text = List.filled(20, sentence).join(' ');
      final spoken = shortenForSpeech(text, limit: 200);
      expect(spoken, endsWith(restOnPhone));
      final kept = spoken.substring(0, spoken.length - restOnPhone.length).trim();
      expect(kept.length, lessThanOrEqualTo(200));
      expect(kept, endsWith('yes.'));
      expect(kept.split(sentence).length - 1, 3);
    });

    test('one endless sentence is cut at a word', () {
      final text = List.filled(100, 'word').join(' ');
      final spoken = shortenForSpeech(text, limit: 50);
      expect(spoken, startsWith('word word'));
      expect(spoken, contains('word… $restOnPhone'));
    });

    test('spokenAnswer cleans before it shortens', () {
      final spoken = spokenAnswer('**Done.** ${List.filled(50, 'More text here.').join(' ')}', limit: 40);
      expect(spoken, startsWith('Done. More text here.'));
      expect(spoken, isNot(contains('*')));
    });
  });

  group('selection', () {
    final boss = employee('boss', reportsTo: null);
    final employees = [
      employee('max', activity: [100, 500]),
      employee('dean', activity: [300]),
      employee('simon', activity: [50]),
      employee('paused', activity: [900], status: LifecycleStatus.paused),
      employee('archived', activity: [1000], status: LifecycleStatus.archived),
      employee('new'),
    ];

    test('employees: the active ones of the whole company, most recently active first', () {
      expect(recentEmployees(employees).map((m) => m.id), ['max', 'dean', 'simon', 'new']);
      expect(employeeActivity(employees.first), 500);
    });

    test('the employee picked before stays, else the single root, else the most recent', () {
      expect(chooseEmployee(employees, 'dean')?.id, 'dean');
      expect(chooseEmployee([boss, ...employees], 'paused')?.id, 'boss', reason: 'the management team has one root');
      expect(chooseEmployee([boss, ...employees], null)?.id, 'boss');
      expect(chooseEmployee([...employees, employee('root2', reportsTo: null), employee('root3', reportsTo: null)], 'deleted')?.id, 'max',
          reason: 'two roots: nobody is preselected, so the most recent');
      expect(chooseEmployee(const [], 'dean'), isNull);
    });

    test('dictated work goes to the project the employee worked in last, else the newest active one', () {
      final projects = [project('a', createdAtMs: 1), project('b', createdAtMs: 2), project('old', status: ProjectStatus.archived, createdAtMs: 3)];
      final tasks = [
        taskOf(id: 't1', projectId: 'a', employeeId: 'max', title: 'x', goal: '', status: TaskStatus.running, updatedAtMs: 5),
        taskOf(id: 't2', projectId: 'old', employeeId: 'max', title: 'x', goal: '', status: TaskStatus.running, updatedAtMs: 9),
      ];
      expect(loggingProject(projects, tasks, 'max')?.id, 'a', reason: 'the archived project is skipped');
      expect(loggingProject(projects, tasks, 'dean')?.id, 'b');
      expect(loggingProject(const [], tasks, 'max'), isNull);
    });

    test('needs you: waiting for the user, newest first', () {
      final tasks = [
        task('input', updatedAtMs: 10),
        task('dependency', waitingReason: 'dependency', updatedAtMs: 99),
        task('approval', status: TaskStatus.running, waitingReason: null, pendingApprovals: 1, updatedAtMs: 30),
        task('escalated', waitingReason: 'escalated', updatedAtMs: 20),
        task('done', status: TaskStatus.succeeded, waitingReason: null, updatedAtMs: 50),
      ];
      expect(needsYouTasks(tasks).map((t) => t.id), ['approval', 'escalated', 'input']);
    });

    test('input is answered by voice; approvals on the phone', () {
      expect(answerableByVoice(task('input')), isTrue);
      expect(answerableByVoice(task('escalated', waitingReason: 'escalated')), isTrue);
      expect(answerableByVoice(task('approval', status: TaskStatus.running, waitingReason: null, pendingApprovals: 2)), isFalse);
      expect(answerableByVoice(task('unassigned', employeeName: null)), isFalse);
      expect(taskPrompt(task('approval', status: TaskStatus.running, waitingReason: null, pendingApprovals: 1)), contains('phone'));
    });

    test('what is read out about waiting tasks', () {
      expect(needsYouSummary(const []), 'Nothing needs you right now.');
      expect(needsYouSummary([task('1')]), 'One task needs you: Task 1, for Max.');
      expect(
        needsYouSummary([task('1'), task('2', employeeName: null), task('3'), task('4'), task('5')]),
        'Five tasks need you: Task 1, for Max; Task 2; Task 3, for Max; and two more.',
      );
      expect(taskIntro(task('7')), 'Task 7, for Max. Waiting for you.');
    });

    test('a dictated task or idea: first sentence as title, all of it as body', () {
      expect(dictatedNote('  fix the login page.  It  breaks on Safari '), (title: 'Fix the login page', body: 'fix the login page. It breaks on Safari'));
      expect(dictatedNote('dark mode for the desktop app'), (title: 'Dark mode for the desktop app', body: 'dark mode for the desktop app'));
      final long = dictatedNote(List.filled(30, 'word').join(' '), limit: 30);
      expect(long.title, 'Word word word word word word…');
      expect(long.body.split(' ').length, 30);
    });
  });
}
