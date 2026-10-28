import '../../data/views.dart';

/// Server feature (`HelloResult.features`): without it neither the quick replies
/// nor the tool that makes them exist.
const quickRepliesFeature = 'quick_replies';

const suggestRepliesTool = 'mcp__aurelius__suggest_replies';
const todoWriteTool = 'TodoWrite';

/// Longest label on a quick-reply chip; the whole text is still what is sent.
const quickReplyLabelMax = 40;
const quickReplyMax = 3;

// ---- Checklist (TodoWrite) ----

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

/// The checklist is not a step of its own: it is drawn as a card.
bool isTodoWrite(ActivityView a) => a.tool == todoWriteTool;

/// Steps that never show as a normal step.
bool isHiddenStep(ActivityView a) => isTodoWrite(a) || a.tool == suggestRepliesTool;

/// The current step for the live status line: the row in progress of the latest checklist.
String? currentStep(List<ActivityView> activity) {
  final todo = activity.lastWhereOrNull(isTodoWrite);
  if (todo == null) return null;
  return parseTodos(todo.input).where((t) => t.status == TodoStatus.inProgress).firstOrNull?.label;
}

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
  const TodoBlock(this.call);
  final ActivityView call;
}

/// Groups the parts of an answer. Steps with nothing written between them share
/// one run; only the last TodoWrite is shown (as a card, where it was called)
/// and suggest_replies never is.
List<AnswerBlock> answerBlocks(List<Object> parts) {
  final lastTodo = parts.whereType<ActivityView>().lastWhereOrNull(isTodoWrite);
  final blocks = <AnswerBlock>[];
  for (final part in parts) {
    if (part is! ActivityView) {
      blocks.add(TextBlock(part as String));
    } else if (isTodoWrite(part)) {
      if (identical(part, lastTodo) && parseTodos(part.input).isNotEmpty) blocks.add(TodoBlock(part));
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
