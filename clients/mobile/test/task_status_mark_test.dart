import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/project/project_bloc.dart';
import 'package:aurelius_mobile/features/project/task_status_mark.dart';
import 'package:aurelius_mobile/features/project/tasks_tab.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/builders.dart';

TaskView task(String id, TaskStatus status, {String? waitingReason, int pendingApprovals = 0, LiveRun? liveRun}) =>
    taskOf(liveRun: liveRun, id: id, title: 'Task $id', goal: '', status: status, waitingReason: waitingReason, pendingApprovals: pendingApprovals);

void main() {
  test('active, waiting and completed tasks are told apart', () {
    final state = ProjectState(tasks: [task('f', TaskStatus.running), task('w', TaskStatus.waiting, waitingReason: 'input'), task('d', TaskStatus.succeeded)]);
    expect(state.filtered(TaskFilter.active).map((t) => t.id), ['f']);
    expect(state.filtered(TaskFilter.waiting).map((t) => t.id), ['w']);
    expect(state.filtered(TaskFilter.completed).map((t) => t.id), ['d']);
  });

  test('a task in Review only says working while a run answers a message', () {
    expect(taskMark(task('r', TaskStatus.review)), TaskMark.inReview);
    expect(taskMark(task('q', TaskStatus.review, liveRun: LiveRun.queued)), TaskMark.queued);
    expect(taskMark(task('w', TaskStatus.review, liveRun: LiveRun.running)), TaskMark.working);
  });

  test('each state has its own mark', () {
    expect(taskMark(task('a', TaskStatus.succeeded)), TaskMark.done);
    expect(taskMark(task('b', TaskStatus.failed)), TaskMark.failed);
    expect(taskMark(task('c', TaskStatus.cancelled)), TaskMark.cancelled);
    expect(taskMark(task('d', TaskStatus.waiting, waitingReason: 'input')), TaskMark.needsYou);
    expect(taskMark(task('e', TaskStatus.running, pendingApprovals: 1)), TaskMark.needsYou);
    expect(taskMark(task('f', TaskStatus.running)), TaskMark.working);
    expect(taskMark(task('f', TaskStatus.running), compacting: true), TaskMark.compacting);
    expect(taskMark(task('a', TaskStatus.succeeded), compacting: true), TaskMark.done);
    expect(taskMark(task('g', TaskStatus.queued)), TaskMark.queued);
    expect(taskMark(task('h', TaskStatus.waiting, waitingReason: 'dependency')), TaskMark.waiting);
    // A usage-limit wait is a calm wait, never Needs you or Failed.
    final quota = task('i', TaskStatus.waiting, waitingReason: 'quota');
    expect(taskMark(quota), TaskMark.waiting);
    expect(taskMarkDetail(quota), 'Waiting for limit reset');
    final now = DateTime(2026, 10, 10, 9);
    expect(limitWait(DateTime(2026, 10, 10, 14, 5).millisecondsSinceEpoch, now: now), 'Waiting for limit reset until 14:05');
    expect(limitWait(DateTime(2026, 10, 11, 3).millisecondsSinceEpoch, now: now), 'Waiting for limit reset until Sun 11 Oct 03:00');
  });

  test('queued tasks that do not start now wait, out of Active', () {
    final line = task('l', TaskStatus.queued, waitingReason: 'in_line');
    final later = task('s', TaskStatus.queued, waitingReason: 'scheduled');
    final now = task('q', TaskStatus.queued);
    expect((line.statusLabel, later.statusLabel, now.statusLabel), ('In line', 'Scheduled', 'Queued'));
    expect((taskMark(line), taskMark(now)), (TaskMark.waiting, TaskMark.queued));
    final state = ProjectState(tasks: [line, later, now]);
    expect(state.filtered(TaskFilter.active).map((t) => t.id), ['q']);
    expect(state.filtered(TaskFilter.waiting).map((t) => t.id), ['l', 's']);
  });

  test('a live follow-up run beats Done, Failed and Cancelled but not Needs you', () {
    expect(taskMark(task('a', TaskStatus.succeeded, liveRun: LiveRun.running)), TaskMark.working);
    expect(taskMark(task('b', TaskStatus.failed, liveRun: LiveRun.queued)), TaskMark.queued);
    expect(taskMark(task('c', TaskStatus.cancelled, liveRun: LiveRun.running)), TaskMark.working);
    expect(taskMark(task('d', TaskStatus.succeeded, liveRun: LiveRun.running, pendingApprovals: 1)), TaskMark.needsYou);
    expect(taskMark(task('e', TaskStatus.succeeded)), TaskMark.done);
    final state = ProjectState(tasks: [task('a', TaskStatus.succeeded, liveRun: LiveRun.running), task('b', TaskStatus.failed, liveRun: LiveRun.queued), task('z', TaskStatus.succeeded)]);
    expect(state.filtered(TaskFilter.active).map((t) => t.id), ['a', 'b']);
    expect(state.filtered(TaskFilter.completed).map((t) => t.id), ['z']);
  });

  testWidgets('the task list marks finished tasks Done at a glance', (tester) async {
    await tester.pumpWidget(MaterialApp(
      home: Scaffold(
        body: ListView(children: [
          TaskRow(task: task('1', TaskStatus.succeeded)),
          TaskRow(task: task('2', TaskStatus.failed)),
          TaskRow(task: task('3', TaskStatus.cancelled)),
          TaskRow(task: task('4', TaskStatus.waiting, waitingReason: 'input')),
          TaskRow(task: task('5', TaskStatus.running)),
        ]),
      ),
    ));
    expect(find.text('Done'), findsOneWidget);
    expect(find.byIcon(Icons.check_circle_rounded), findsOneWidget);
    expect(find.text('Failed'), findsOneWidget);
    expect(find.text('Cancelled'), findsOneWidget);
    expect(find.text('Needs you'), findsOneWidget);
    expect(find.text(' · Waiting for you'), findsOneWidget, reason: 'what it waits for stays beside the mark');
    expect(find.text('Working'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
