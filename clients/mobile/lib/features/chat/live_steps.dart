import '../../data/views.dart';

/// Server feature (`HelloResult.features`): without it neither the quick replies
/// nor the tool that makes them exist.
const quickRepliesFeature = 'quick_replies';

const suggestRepliesTool = 'mcp__aurelius__suggest_replies';
const todoWriteTool = 'TodoWrite';
const taskCreateTool = 'TaskCreate';
const taskUpdateTool = 'TaskUpdate';

/// Claude Code's own todo list: `TodoWrite` before CLI 2.1.318, the Task* tools since.
const _todoTools = {todoWriteTool, taskCreateTool, taskUpdateTool, 'TaskList', 'TaskGet'};

/// Longest label on a quick-reply chip; the whole text is still what is sent.
const quickReplyLabelMax = 40;
const quickReplyMax = 3;

// ---- Checklist (TodoWrite, TaskCreate/TaskUpdate) ----

enum TodoStatus { pending, inProgress, completed }

class TodoItem {
  const TodoItem(this.content, this.status, this.activeForm);

  final String content;
  final TodoStatus status;

  /// What is being done, while it is in progress ("Running the tests").
  final String activeForm;

  /// The row's text: the active form while it runs.
  String get label => status == TodoStatus.inProgress && activeForm.isNotEmpty ? activeForm : content;
}

/// The rows of a TodoWrite call's input (`todos`); unreadable rows are skipped.
List<TodoItem> parseTodos(Object? input) {
  final todos = input is Map ? input['todos'] : null;
  if (todos is! List) return const [];
  return [
    for (final t in todos)
      if (t is Map && (t['content'] is String || t['activeForm'] is String))
        TodoItem(
          (t['content'] as String?) ?? (t['activeForm'] as String),
          switch (t['status']) {
            'completed' => TodoStatus.completed,
            'in_progress' => TodoStatus.inProgress,
            _ => TodoStatus.pending,
          },
          (t['activeForm'] as String?) ?? '',
        ),
  ];
}

/// A call that builds the checklist: it is not a step of its own, the list is drawn as a card.
bool isTodoCall(ActivityView a) => _todoTools.contains(a.tool);

/// Steps that never show as a normal step.
bool isHiddenStep(ActivityView a) => isTodoCall(a) || a.tool == suggestRepliesTool;

final _taskNumber = RegExp(r'#(\d+)');

String? _text(Object? v) => v is String && v.trim().isNotEmpty ? v.trim() : null;

TodoStatus _status(Object? v) => switch (v) {
      'completed' => TodoStatus.completed,
      'in_progress' => TodoStatus.inProgress,
      _ => TodoStatus.pending,
    };

/// The checklist over answers, oldest first, or null when there is none. Since
/// Claude Code 2.1.318 a run makes it one call at a time: TaskCreate numbers its
/// items 1, 2, 3 (its result says the number) and TaskUpdate changes one later,
/// often in a later answer, so the list carries over from answer to answer. A
/// TodoWrite (older CLIs) replaces the whole list. Numbering that starts over is
/// a new session's list. Offsets count per answer, so each is sorted on its own.
List<TodoItem>? todosAcross(Iterable<List<ActivityView>> answers) {
  List<TodoItem>? snapshot;
  Map<String, TodoItem>? tasks;
  var top = 0;
  for (final activity in answers) {
    for (final a in [...activity]..sort((x, y) => x.offset.compareTo(y.offset))) {
      if (a.ok == false) continue;
      final input = a.input is Map ? a.input as Map : const {};
      if (a.tool == todoWriteTool) {
        if (input['todos'] is! List) continue;
        snapshot = parseTodos(input);
        tasks = null;
      } else if (a.tool == taskCreateTool) {
        final id = _taskNumber.firstMatch(a.output ?? '')?.group(1) ?? '${top + 1}';
        if (tasks == null || tasks.containsKey(id) || (id == '1' && top > 0)) {
          tasks = {};
          top = 0;
        }
        snapshot = null;
        final subject = _text(input['subject']);
        final active = _text(input['activeForm']);
        if (subject != null || active != null) tasks[id] = TodoItem(subject ?? active!, TodoStatus.pending, active ?? '');
        top = top > (int.tryParse(id) ?? 0) ? top : (int.tryParse(id) ?? 0);
      } else if (a.tool == taskUpdateTool && tasks != null) {
        final id = '${input['taskId'] ?? ''}';
        final item = tasks[id];
        if (item == null) continue;
        if (input['status'] == 'deleted') {
          tasks.remove(id);
          continue;
        }
        tasks[id] = TodoItem(
          _text(input['subject']) ?? item.content,
          input['status'] is String ? _status(input['status']) : item.status,
          _text(input['activeForm']) ?? item.activeForm,
        );
      }
    }
  }
  final todos = snapshot ?? tasks?.values.toList() ?? const <TodoItem>[];
  return todos.isEmpty ? null : todos;
}

/// The checklist as it stands after each answer that changed it, by message id.
/// Each employee keeps its own list, so in a group chat the answers fold per author.
Map<String, List<TodoItem>> todosByAnswer(List<MessageView> messages) {
  final seen = <String, List<List<ActivityView>>>{};
  final out = <String, List<TodoItem>>{};
  for (final m in messages) {
    if (m.isUser || m.update != null || m.system != null) continue;
    final answers = seen.putIfAbsent(m.author ?? '', () => []);
    answers.add(m.activity);
    if (!m.activity.any(isTodoCall)) continue;
    final todos = todosAcross(answers);
    if (todos != null) out[m.id] = todos;
  }
  return out;
}

/// The row in progress of a checklist, for the live status line.
String? currentStep(List<TodoItem>? todos) => todos?.where((t) => t.status == TodoStatus.inProgress).firstOrNull?.label;

/// What AnswerBody draws, in order: text, runs of steps and the checklist.
sealed class AnswerBlock {
  const AnswerBlock();
}

final class TextBlock extends AnswerBlock {
  const TextBlock(this.markdown);
  final String markdown;
}

final class StepsBlock extends AnswerBlock {
  StepsBlock(this.steps);
  final List<ActivityView> steps;
}

final class TodoBlock extends AnswerBlock {
  const TodoBlock(this.todos);
  final List<TodoItem> todos;
}

/// Groups the parts of an answer. Steps with nothing written between them share
/// one run; the checklist shows once, as [todos] (the list after this answer),
/// where its last call was; suggest_replies never shows. Without [todos] the
/// list comes from this answer's own calls.
List<AnswerBlock> answerBlocks(List<Object> parts, {List<TodoItem>? todos}) {
  final calls = parts.whereType<ActivityView>();
  final lastTodo = calls.lastWhereOrNull(isTodoCall);
  final list = todos ?? todosAcross([calls.toList()]);
  final blocks = <AnswerBlock>[];
  for (final part in parts) {
    if (part is! ActivityView) {
      blocks.add(TextBlock(part as String));
    } else if (isTodoCall(part)) {
      if (identical(part, lastTodo) && list != null && list.isNotEmpty) blocks.add(TodoBlock(list));
    } else if (isHiddenStep(part)) {
      continue;
    } else if (blocks.isNotEmpty && blocks.last is StepsBlock) {
      (blocks.last as StepsBlock).steps.add(part);
    } else {
      blocks.add(StepsBlock([part]));
    }
  }
  return blocks;
}

// ---- Quick replies ----

class QuickReplies {
  const QuickReplies({required this.messageId, required this.question, required this.options});

  /// The answer the suggestions belong to.
  final String messageId;
  final String question;

  /// The texts to send (at most [quickReplyMax]).
  final List<String> options;
}

String quickReplyLabel(String text) => text.length <= quickReplyLabelMax ? text : '${text.substring(0, quickReplyLabelMax - 1).trimRight()}…';

/// The suggestions in a suggest_replies call, or null when it has fewer than two usable options.
QuickReplies? parseQuickReplies(ActivityView call, String messageId) {
  final input = call.input;
  if (input is! Map) return null;
  final raw = input['options'];
  final options = [
    if (raw is List)
      for (final o in raw)
        if (o is String && o.trim().isNotEmpty) o.trim(),
  ].take(quickReplyMax).toList();
  if (options.length < 2) return null;
  final question = input['question'];
  return QuickReplies(messageId: messageId, question: question is String ? question.trim() : '', options: options);
}

/// The suggestions to show under the last answer, or null. They go away with an
/// unsent draft, a new message or run, a pending approval, a plan, or when dismissed.
QuickReplies? quickRepliesToShow({
  required List<MessageView> messages,
  required bool enabled,
  required bool hasUnsent,
  required bool blocked,
  required String draft,
  String? dismissedId,
}) {
  if (!enabled || hasUnsent || blocked || draft.trim().isNotEmpty || messages.isEmpty) return null;
  final last = messages.last;
  if (last.isUser || last.update != null || last.status != MessageStatus.complete || last.id == dismissedId) return null;
  if (last.activity.any((a) => a.tool.endsWith('ExitPlanMode'))) return null;
  final call = last.activity.lastWhereOrNull((a) => a.tool == suggestRepliesTool);
  return call == null ? null : parseQuickReplies(call, last.id);
}

extension _LastWhere<T> on Iterable<T> {
  T? lastWhereOrNull(bool Function(T) test) {
    T? found;
    for (final e in this) {
      if (test(e)) found = e;
    }
    return found;
  }
}
