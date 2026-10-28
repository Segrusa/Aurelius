import 'package:flutter/material.dart';

import '../../core/net/compacting.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../data/views.dart';

/// How a task stands at a glance, in the same words on every screen.
enum TaskMark { done, failed, cancelled, needsYou, working, compacting, queued, waiting, draft }

TaskMark taskMark(TaskView t, {bool compacting = false}) {
  if (t.needsYou) return TaskMark.needsYou;
  if (t.waitsToStart) return TaskMark.waiting;
  if (compacting && t.status == TaskStatus.running) return TaskMark.compacting;
  if (t.hasLiveRun) return t.liveRun == LiveRun.queued ? TaskMark.queued : TaskMark.working;
  return switch (t.status) {
    TaskStatus.succeeded => TaskMark.done,
    TaskStatus.failed => TaskMark.failed,
    TaskStatus.cancelled => TaskMark.cancelled,
    TaskStatus.running || TaskStatus.review => TaskMark.working,
    TaskStatus.queued => TaskMark.queued,
    TaskStatus.waiting => TaskMark.waiting,
    TaskStatus.draft => TaskMark.draft,
  };
}

/// What [TaskStatusMark] leaves out: why a task waits; null when the mark says it all.
String? taskMarkDetail(TaskView t) => switch (taskMark(t)) {
      TaskMark.needsYou || TaskMark.waiting => t.statusLabel,
      _ => null,
    };

/// A compact status mark: green check + "Done", red "Failed", grey
/// "Cancelled", accent "Needs you" pill, a breathing dot while working.
class TaskStatusMark extends StatelessWidget {
  const TaskStatusMark(this.task, {super.key, this.size = 12.5});

  final TaskView task;
  final double size;

  @override
  Widget build(BuildContext context) {
    final conversation = task.conversationId;
    if (conversation == null || task.status != TaskStatus.running || task.hasLiveRun) return _build(context, taskMark(task));
    return ValueListenableBuilder<Set<String>>(
      valueListenable: compactingConversations,
      builder: (context, compacting, _) => _build(context, taskMark(task, compacting: compacting.contains(conversation))),
    );
  }

  Widget _build(BuildContext context, TaskMark mark) {
    if (mark == TaskMark.needsYou) {
      return Container(
        key: const ValueKey('task-mark-needsYou'),
        padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 1),
        decoration: BoxDecoration(color: Tokens.accentTint, borderRadius: BorderRadius.circular(999)),
        child: Row(mainAxisSize: MainAxisSize.min, children: [
          Icon(Icons.priority_high_rounded, size: size + 1, color: Tokens.accent),
          const SizedBox(width: 2),
          Text('Needs you', style: TextStyle(color: Tokens.accent, fontSize: size, fontWeight: FontWeight.w600)),
        ]),
      );
    }
    final (Widget icon, String label, Color color, FontWeight weight) = switch (mark) {
      TaskMark.done => (Icon(Icons.check_circle_rounded, size: size + 3, color: Tokens.accent), 'Done', Tokens.accent, FontWeight.w600),
      TaskMark.failed => (Icon(Icons.cancel_rounded, size: size + 3, color: Tokens.negative), 'Failed', Tokens.negative, FontWeight.w600),
      TaskMark.cancelled => (Icon(Icons.block_rounded, size: size + 2, color: Tokens.muted), 'Cancelled', Tokens.muted, FontWeight.w500),
      TaskMark.working => (StatusDot(pulse: true, size: size * 0.6), task.status == TaskStatus.review ? 'In review' : 'Working', Tokens.text2, FontWeight.w500),
      TaskMark.compacting => (StatusDot(pulse: true, size: size * 0.6), compactingLabel, Tokens.accent, FontWeight.w600),
      TaskMark.queued => (Icon(Icons.schedule_rounded, size: size + 2, color: Tokens.text2), 'Queued', Tokens.text2, FontWeight.w500),
      TaskMark.waiting => (Icon(Icons.hourglass_empty_rounded, size: size + 1, color: Tokens.muted), 'Waiting', Tokens.muted, FontWeight.w500),
      TaskMark.draft || TaskMark.needsYou => (Icon(Icons.edit_note_rounded, size: size + 3, color: Tokens.muted), 'Draft', Tokens.muted, FontWeight.w500),
    };
    return Row(
      key: ValueKey('task-mark-${mark.name}'),
      mainAxisSize: MainAxisSize.min,
      children: [
        SizedBox(width: size + 3, child: Center(child: icon)),
        const SizedBox(width: 4),
        Text(label, style: TextStyle(color: color, fontSize: size, fontWeight: weight)),
      ],
    );
  }
}
