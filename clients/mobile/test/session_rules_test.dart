import 'package:aurelius_mobile/data/protocol/protocol.dart';
import 'package:aurelius_mobile/features/sessions/session_rules.dart';
import 'package:flutter_test/flutter_test.dart';

WorkSessionView session({String id = 'c1', bool running = false, SessionDispatch? dispatch, int at = 0, String? branch, String? worktree}) => WorkSessionView(
      conversationId: id,
      createdAtMs: 0,
      employeeId: 'e1',
      employeeName: 'Forge',
      lastActivityMs: at,
      projectId: 'p1',
      projectName: 'Aurelius',
      running: running,
      title: 'Fix the login',
      dispatch: dispatch,
      branch: branch,
      worktree: worktree,
    );

Receipt receipt(CommandResult value) => Receipt(committedSeq: 1, idempotencyKey: 'k', replayed: false, result: WriteResultCommand(value: value));

void main() {
  group('sessionSignal', () {
    test('a running session works', () => expect(sessionSignal(session(running: true)), SessionSignal.working));
    test('a session the dispatcher is choosing for works', () => expect(sessionSignal(session(dispatch: SessionDispatch.choosing)), SessionSignal.working));
    test('a session the dispatcher chose nobody for needs you', () => expect(sessionSignal(session(dispatch: SessionDispatch.nobody)), SessionSignal.needsYou));
    test('a quiet session has no signal', () => expect(sessionSignal(session()), isNull));
  });

  test('activeSessions counts only the working ones', () {
    expect(activeSessions([session(running: true), session(dispatch: SessionDispatch.choosing), session(dispatch: SessionDispatch.nobody), session()]), 2);
  });

  test('sessionsByActivity puts the newest activity first', () {
    final sorted = sessionsByActivity([session(id: 'a', at: 1), session(id: 'b', at: 3), session(id: 'c', at: 2)]);
    expect(sorted.map((s) => s.conversationId), ['b', 'c', 'a']);
  });

  group('sessionPeriod', () {
    // Wednesday 18 November 2026, 15:00 local.
    final now = DateTime(2026, 11, 18, 15);
    int at(int year, int month, int day, [int hour = 0]) => DateTime(year, month, day, hour).millisecondsSinceEpoch;

    test('a working session is Active whenever it last moved', () => expect(sessionPeriod(session(running: true, at: at(2025, 1, 1)), now), SessionPeriod.active));
    test('a session waiting for your choice is Active', () => expect(sessionPeriod(session(dispatch: SessionDispatch.nobody, at: at(2025, 1, 1)), now), SessionPeriod.active));
    test('since midnight is Today', () => expect(sessionPeriod(session(at: at(2026, 11, 18)), now), SessionPeriod.today));
    test('the day before is Yesterday', () => expect(sessionPeriod(session(at: at(2026, 11, 17, 23)), now), SessionPeriod.yesterday));
    test('Monday of this week is This week', () => expect(sessionPeriod(session(at: at(2026, 11, 16)), now), SessionPeriod.thisWeek));
    test('the Sunday before is This month', () => expect(sessionPeriod(session(at: at(2026, 11, 15, 23)), now), SessionPeriod.thisMonth));
    test('before the first of the month is Older', () => expect(sessionPeriod(session(at: at(2026, 10, 31, 23)), now), SessionPeriod.older));
    test('on a Monday, Yesterday wins over This week', () => expect(sessionPeriod(session(at: at(2026, 11, 15, 12)), DateTime(2026, 11, 16, 9)), SessionPeriod.yesterday));

    test('sessionSections keeps the order and drops empty sections', () {
      final sections = sessionSections([
        session(id: 'old', at: at(2026, 9, 1)),
        session(id: 'today2', at: at(2026, 11, 18, 10)),
        session(id: 'run', running: true, at: at(2026, 11, 1)),
        session(id: 'today1', at: at(2026, 11, 18, 9)),
      ], now);
      expect(sections.map((s) => s.$1), [SessionPeriod.active, SessionPeriod.today, SessionPeriod.older]);
      expect(sections[1].$2.map((s) => s.conversationId), ['today2', 'today1']);
    });
  });

  group('words', () {
    test('sessionWho says who works in it', () {
      expect(sessionWho(session()), 'Forge');
      expect(sessionWho(session(dispatch: SessionDispatch.choosing)), 'Forge is choosing who takes it');
      expect(sessionWho(session(dispatch: SessionDispatch.nobody)), 'Nobody yet');
    });

    test('sessionStatus follows the run and the dispatcher', () {
      expect(sessionStatus(true, null), 'Working');
      expect(sessionStatus(false, null), 'Waiting for you');
      expect(sessionStatus(true, SessionDispatch.choosing), 'Dispatcher is choosing');
      expect(sessionStatus(false, SessionDispatch.nobody), 'Waiting for your choice');
    });

    test('chooserText only while nobody works in it', () {
      expect(chooserText(session()), isNull);
      expect(chooserText(session(dispatch: SessionDispatch.choosing)), 'Forge is choosing who takes this session. You can also choose yourself.');
      expect(chooserText(session(dispatch: SessionDispatch.nobody)), 'The dispatcher chose nobody. Choose who takes this session.');
    });

    test('sessionFolder is the worktree or the project folder', () {
      expect(sessionFolder(session()), 'Project folder');
      expect(sessionFolder(session(worktree: '/w/t')), '/w/t');
    });

    test('sessionNote and dispatchedNote as on the desktop', () {
      expect(sessionNote('Forge', false), "Forge works in the project's folder itself. No ticket: you talk with them in the session.");
      expect(sessionNote(null, true), 'The employee works in a worktree of its own, on its own branch. No ticket: you talk with them in the session.');
      expect(dispatchedNote(false), "The dispatcher chooses who takes it, in the project's folder itself. No ticket: you talk with them in the session.");
    });
  });

  group('start params', () {
    const start = SessionStartInput(projectId: 'p1', text: '  Fix it  ', model: 'default', effort: 'high', mode: 'plan', images: ['i.png'], files: ['a.pdf']);

    test('sessionStartParams trims the text, drops default picks and carries photos and files', () {
      final p = sessionStartParams(start, employeeId: 'e1', confirmSecret: false).toJson();
      expect(p, {
        'confirm_secret': false,
        'employee_id': 'e1',
        'files': ['a.pdf'],
        'images': ['i.png'],
        'project_id': 'p1',
        'run': {'effort': 'high', 'mode': 'plan'},
        'text': 'Fix it',
        'worktree': false,
      });
    });

    test('all defaults send no run', () {
      const plain = SessionStartInput(projectId: 'p1', text: 'x', model: 'default', effort: 'default', mode: 'default');
      expect(sessionStartParams(plain, employeeId: 'e1', confirmSecret: true).toJson().containsKey('run'), isFalse);
    });

    test('sessionDispatchParams is the start without an employee', () {
      final p = sessionDispatchParams(start, confirmSecret: true).toJson();
      expect(p.containsKey('employee_id'), isFalse);
      expect(p['text'], 'Fix it');
      expect(p['files'], ['a.pdf']);
      expect(p['confirm_secret'], true);
    });
  });

  group('startedSession', () {
    test('reads the conversation of a started session', () {
      expect(startedSession(receipt(const CommandResultWorkSessionStarted(conversationId: 'c9', messageId: 'm', projectId: 'p', runId: 'r'))), 'c9');
    });
    test('reads the conversation of a session the dispatcher gives away', () {
      expect(startedSession(receipt(const CommandResultWorkSessionDispatching(conversationId: 'c8', messageId: 'm', projectId: 'p'))), 'c8');
    });
    test('is null for anything else', () {
      expect(startedSession(receipt(const CommandResultRunDefaultsSet())), isNull);
    });
  });

  group('startProblem', () {
    test('asks for a connection, a project, text and someone, in that order', () {
      expect(startProblem(offline: true, projectId: 'p', text: 'x', who: 'e1', dispatchOn: false), 'Not connected to the server.');
      expect(startProblem(offline: false, projectId: null, text: 'x', who: 'e1', dispatchOn: false), 'Choose a project.');
      expect(startProblem(offline: false, projectId: 'p', text: ' ', who: 'e1', dispatchOn: false), 'Describe what needs doing.');
      expect(startProblem(offline: false, projectId: 'p', text: 'x', who: null, dispatchOn: false), 'Choose who does it.');
      expect(startProblem(offline: false, projectId: 'p', text: 'x', who: 'e1', dispatchOn: false), isNull);
    });

    test('the Dispatcher counts only on a server with session_dispatch', () {
      expect(startProblem(offline: false, projectId: 'p', text: 'x', who: dispatcherWho, dispatchOn: false), 'Choose who does it.');
      expect(startProblem(offline: false, projectId: 'p', text: 'x', who: dispatcherWho, dispatchOn: true), isNull);
    });

    test('files need chat_files', () {
      expect(startProblem(offline: false, projectId: 'p', text: 'x', who: 'e1', dispatchOn: false, files: 1, filesOn: false), 'Add files in the session once it has started.');
      expect(startProblem(offline: false, projectId: 'p', text: 'x', who: 'e1', dispatchOn: false, files: 1, filesOn: true), isNull);
    });
  });

  group('defaults', () {
    test('keptChoice: a remembered choice while still offered, else default; kept while the list has not loaded', () {
      expect(keptChoice('high', ['low', 'high']), 'high');
      expect(keptChoice('gone', ['m-1']), 'default');
      expect(keptChoice(null, ['m-1']), 'default');
      expect(keptChoice('m-2', const [], loaded: false), 'm-2');
    });

    test('defaultWho: the last employee while active, else the Dispatcher where there is one', () {
      expect(defaultWho(last: 'e1', active: {'e1', 'e2'}, dispatchOn: true), 'e1');
      expect(defaultWho(last: 'gone', active: {'e1'}, dispatchOn: true), dispatcherWho);
      expect(defaultWho(last: 'gone', active: {'e1'}, dispatchOn: false), isNull);
      expect(defaultWho(last: dispatcherWho, active: {'e1'}, dispatchOn: false), isNull);
    });

    test('defaultSessionProject: the last used while open, else the only one', () {
      expect(defaultSessionProject(['p1', 'p2'], 'p2'), 'p2');
      expect(defaultSessionProject(['p1', 'p2'], 'gone'), isNull);
      expect(defaultSessionProject(['p1'], null), 'p1');
    });

    test('remembered per server', () => expect(sessionKey('project', 'h1'), 'new_session.project:h1'));
  });
}
