import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';
import '../connection/connection_bloc.dart';
import '../markdown/links.dart';
import 'chat_view.dart';
import 'collapse.dart';
import 'live_steps.dart';
import 'screenshot_page.dart';

/// An answer: Markdown with the tools it used shown in place, as separate
/// blocks (commands, edits, web searches), as on the desktop.
class AnswerBody extends StatelessWidget {
  const AnswerBody({super.key, required this.message, this.cut});

  final MessageView message;

  /// Show only the text before this UTF-16 index, and the steps taken before it.
  final int? cut;

  /// The parts before [cut].
  List<Object> _parts() {
    final cut = this.cut;
    if (cut == null) return message.parts;
    final text = message.text;
    final steps = [for (final a in message.activity) if (utf16Index(text, a.offset) <= cut) a];
    return message.withText(collapsedMarkdown(text, cut), activity: steps).parts;
  }

  @override
  Widget build(BuildContext context) {
    final style = markdownStyle(context);
    // Steps with nothing written between them share one line; the checklist is a card.
    final blocks = answerBlocks(_parts());
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (final (i, block) in blocks.indexed)
          Padding(
            padding: EdgeInsets.only(top: i == 0 ? 0 : 10),
            child: switch (block) {
              StepsBlock(steps: [final only]) => ActivityCard(key: ValueKey(only.callId), activity: only, messageId: message.id),
              StepsBlock(steps: final run) => ActivityRun(key: ValueKey('run-${run.first.callId}'), run: run, messageId: message.id),
              TodoBlock(:final call) => TodoCard(key: const Key('todo-card'), todos: parseTodos(call.input)),
              TextBlock(:final markdown) => MessageMarkdown(data: markdown, scope: chatFileScope(context), styleSheet: style, diagrams: !message.isStreaming),
            },
          ),
      ],
    );
  }
}

/// The agent's checklist (TodoWrite): done, in progress (bold, in its active
/// form) and open. Long lists fold to the row in progress and a count.
class TodoCard extends StatefulWidget {
  const TodoCard({super.key, required this.todos});

  final List<TodoItem> todos;

  /// More rows than this fold.
  static const foldAbove = 8;

  @override
  State<TodoCard> createState() => _TodoCardState();
}

class _TodoCardState extends State<TodoCard> {
  bool _expanded = false;

  @override
  Widget build(BuildContext context) {
    final todos = widget.todos;
    if (todos.isEmpty) return const SizedBox.shrink();
    final folds = todos.length > TodoCard.foldAbove;
    final doing = todos.where((t) => t.status == TodoStatus.inProgress).toList();
    final shown = folds && !_expanded ? (doing.isEmpty ? todos.take(1).toList() : doing) : todos;
    final done = todos.where((t) => t.status == TodoStatus.completed).length;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      decoration: BoxDecoration(borderRadius: BorderRadius.circular(8), border: Border.all(color: Tokens.border)),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text('$done of ${todos.length} done', style: TextStyle(fontSize: 11.5, color: Tokens.muted)),
          for (final t in shown)
            Padding(
              padding: const EdgeInsets.only(top: 4),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  SizedBox(
                    width: 20,
                    child: Text(
                      switch (t.status) { TodoStatus.completed => '✓', TodoStatus.inProgress => '◐', TodoStatus.pending => '○' },
                      style: TextStyle(fontSize: 14.5, height: 1.35, color: t.status == TodoStatus.pending ? Tokens.muted : Tokens.text2),
                    ),
                  ),
                  Expanded(
                    child: Text(
                      t.label,
                      style: TextStyle(
                        fontSize: 14.5,
                        height: 1.35,
                        fontWeight: t.status == TodoStatus.inProgress ? FontWeight.w700 : FontWeight.w400,
                        color: t.status == TodoStatus.completed ? Tokens.muted : Tokens.text2,
                        decoration: t.status == TodoStatus.completed ? TextDecoration.lineThrough : null,
                      ),
                    ),
                  ),
                ],
              ),
            ),
          if (folds)
            InkWell(
              key: const Key('todo-fold'),
              onTap: () => setState(() => _expanded = !_expanded),
              child: Padding(
                padding: const EdgeInsets.only(top: 6),
                child: Text(_expanded ? 'Show less' : '${todos.length - shown.length} more', style: TextStyle(fontSize: 12.5, color: Tokens.accentText)),
              ),
            ),
        ],
      ),
    );
  }
}

/// Steps taken one after another without a word in between: one line that
/// shows the latest, with a count to unfold them all.
class ActivityRun extends StatefulWidget {
  const ActivityRun({super.key, required this.run, this.messageId});

  final List<ActivityView> run;
  final String? messageId;

  @override
  State<ActivityRun> createState() => _ActivityRunState();
}

class _ActivityRunState extends State<ActivityRun> {
  bool _expanded = false;

  @override
  Widget build(BuildContext context) {
    final run = widget.run;
    final failed = run.where((a) => a.ok == false).length;
    final toggle = InkWell(
      onTap: () => setState(() => _expanded = !_expanded),
      borderRadius: BorderRadius.circular(5),
      child: Container(
        margin: const EdgeInsets.only(top: 4),
        padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
        decoration: BoxDecoration(borderRadius: BorderRadius.circular(5), border: Border.all(color: Tokens.border)),
        child: Row(mainAxisSize: MainAxisSize.min, children: [
          Text(failed > 0 ? '${run.length} steps · $failed failed' : '${run.length} steps', style: TextStyle(fontSize: 11.5, color: failed > 0 ? Tokens.negative : Tokens.muted)),
          Icon(_expanded ? Icons.expand_more : Icons.chevron_right, size: 14, color: Tokens.muted),
        ]),
      ),
    );
    if (_expanded) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Align(alignment: Alignment.centerLeft, child: toggle),
          for (final a in run) Padding(padding: const EdgeInsets.only(top: 10), child: ActivityCard(key: ValueKey(a.callId), activity: a, messageId: widget.messageId)),
        ],
      );
    }
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        toggle,
        const SizedBox(width: 10),
        Expanded(child: ActivityCard(key: ValueKey(run.last.callId), activity: run.last, messageId: widget.messageId)),
      ],
    );
  }
}

/// One tool use, as Claude Code shows it: what it did, in words, on top;
/// tap to unfold the tool, its command and its output.
class ActivityCard extends StatefulWidget {
  const ActivityCard({super.key, required this.activity, this.messageId});

  final ActivityView activity;

  /// The answer it belongs to, to fetch it whole when it was shortened.
  final String? messageId;

  @override
  State<ActivityCard> createState() => _ActivityCardState();
}

class _ActivityCardState extends State<ActivityCard> {
  bool _expanded = false;

  /// The whole call (`activity.get`), once the user asked for it.
  ActivityView? _full;
  Future<ActivityView?>? _fetching;

  @override
  void didUpdateWidget(ActivityCard old) {
    super.didUpdateWidget(old);
    // Fetched while it ran: the finished call has more.
    if (old.activity.ok != widget.activity.ok) _full = null;
  }

  /// The whole call, fetched once; null (after saying why) when it could not be.
  Future<ActivityView?> _fetch() async {
    if (_full case final full?) return full;
    final fetching = _fetching ??= _load();
    setState(() {});
    try {
      return await fetching;
    } finally {
      _fetching = null;
      if (mounted) setState(() {});
    }
  }

  Future<ActivityView?> _load() async {
    try {
      final full = await context.read<AureliusRepository>().activity(widget.messageId!, widget.activity.callId);
      if (mounted) setState(() => _full = full);
      return full;
    } on Object catch (e) {
      if (mounted) {
        toast(context, switch (e) {
          ProtocolException(code: ErrorCode.notFound) => 'This tool call is no longer kept on the PC.',
          ProtocolException(:final shown) => shown,
          _ => e.toString(),
        });
      }
      return null;
    }
  }

  static String _short(String path) {
    final parts = path.split(RegExp(r'[\\/]')).where((p) => p.isNotEmpty);
    return parts.isEmpty ? path : parts.last;
  }

  /// (what it did, the tool's name, what it ran on), as on the desktop.
  (String, String, String) _label(ActivityView a) {
    String firstLine(String t) => t.split('\n').first;
    String described(String fallback) => a.field('description').trim().isEmpty ? fallback : a.field('description').trim();
    return switch (a.tool) {
      'Bash' || 'PowerShell' => (described(firstLine(a.field('command'))), a.tool, a.field('command')),
      'Edit' || 'MultiEdit' => ('Edited ${_short(a.field('file_path'))}', 'Edit', a.field('file_path')),
      'Write' => ('Wrote ${_short(a.field('file_path'))}', 'Write', a.field('file_path')),
      'NotebookEdit' => ('Edited ${_short(a.field('notebook_path'))}', 'Edit', a.field('notebook_path')),
      'Read' => ('Read ${_short(a.field('file_path'))}', 'Read', a.field('file_path')),
      'Glob' => ('Looked for files: ${a.field('pattern')}', 'Glob', a.field('pattern')),
      'Grep' => ('Searched in files: ${a.field('pattern')}', 'Grep', a.field('pattern')),
      'WebSearch' => ('Searched the web: ${a.field('query')}', 'Web search', a.field('query')),
      'WebFetch' => ('Opened ${a.field('url')}', 'Web fetch', a.field('url')),
      'Task' || 'Agent' => (described('A subagent'), 'Subagent', a.field('prompt')),
      'mcp__aurelius__screenshot' => (switch ((a.field('window'), a.inputValue('monitor'))) {
          (final w, _) when w.isNotEmpty => 'Screenshot of window "$w"',
          (_, final num n) => 'Screenshot of monitor $n',
          _ => 'Screenshot of the screen',
        }, 'Screenshot', ''),
      'mcp__aurelius__list_windows' => ('Looked at the open windows', 'Windows', ''),
      'mcp__aurelius__remember' => ('Noted in memory', 'Memory', a.field('content')),
      'mcp__aurelius__recall' => ('Looked in memory: ${a.field('query')}', 'Memory', a.field('query')),
      'mcp__aurelius__delegate_task' => ('Gave "${a.field('title')}" to ${a.field('employee')}', 'Delegate', a.field('goal')),
      'mcp__aurelius__hand_over_task' => ('Handed the task to ${a.field('employee')}', 'Hand over', a.field('reason')),
      'mcp__aurelius__propose_colleague' => ('Proposed a colleague: ${a.field('name')}', 'Colleague', a.field('reason')),
      'mcp__aurelius__add_idea' => ('Noted an idea: ${a.field('title')}', 'Idea', a.field('note')),
      'mcp__aurelius__list_tasks' => ("Looked at the project's tasks", 'Tasks', ''),
      'mcp__aurelius__read_task' => ('Read the task ${a.field('task')}', 'Task', ''),
      'mcp__aurelius__list_conversations' => ('Looked at its other conversations', 'Conversations', ''),
      'mcp__aurelius__read_conversation' => ('Read the conversation ${a.field('conversation')}', 'Conversation', ''),
      final other => (described(other.replaceFirst('mcp__aurelius__', '').replaceAll('_', ' ')), other, ''),
    };
  }

  @override
  Widget build(BuildContext context) {
    final a = _full ?? widget.activity;
    final (summary, tool, code) = _label(a);
    final output = a.tool == 'WebSearch' ? webSearchSummary(a.output ?? '') : (a.output ?? '');
    final shot = a.tool == 'mcp__aurelius__screenshot' ? screenshotId(a.output) : null;
    final showOutput = output.trim().isNotEmpty && !const {'Read', 'Edit', 'MultiEdit', 'Write', 'mcp__aurelius__screenshot'}.contains(a.tool);
    final lines = output.split('\n');
    final shown = lines.length.clamp(0, 400);
    final more = lines.length - shown;
    final failed = a.ok == false;
    // Shortened by the server (`activity_preview`) in a part this card shows.
    final cut = a.truncated == true &&
        ((showOutput && a.outputBytes != null) || utf8.encode(code).length >= _inputCap) &&
        widget.messageId != null &&
        context.select<ConnectionBloc, bool>((b) => b.state.has('activity_preview'));
    final outputCut = cut && showOutput && a.outputBytes != null;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        InkWell(
          onTap: () => setState(() => _expanded = !_expanded),
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 4),
            child: Row(
              children: [
                Flexible(
                  child: Text(summary, maxLines: 2, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 14.5, height: 1.35, color: failed ? Tokens.negative : Tokens.text2)),
                ),
                const SizedBox(width: 4),
                Icon(_expanded ? Icons.expand_more : Icons.chevron_right, size: 18, color: Tokens.muted),
                const Spacer(),
                if (shot != null) ...[
                  OutlinedButton(
                    key: const Key('view-screenshot'),
                    style: OutlinedButton.styleFrom(visualDensity: VisualDensity.compact, padding: const EdgeInsets.symmetric(horizontal: 10)),
                    onPressed: () => ScreenshotPage.open(context, shot),
                    child: const Text('View'),
                  ),
                  const SizedBox(width: 8),
                ],
                switch (a.ok) {
                  null => Row(mainAxisSize: MainAxisSize.min, children: [
                      SizedBox.square(dimension: 10, child: CircularProgressIndicator(strokeWidth: 1.5)),
                      SizedBox(width: 6),
                      Text('running', style: TextStyle(fontSize: 11.5, color: Tokens.muted)),
                    ]),
                  true => Text('✓', style: TextStyle(fontSize: 12, color: Tokens.accentText)),
                  false => Text('✗ failed', style: TextStyle(fontSize: 12, color: Tokens.negative)),
                },
              ],
            ),
          ),
        ),
        if (_expanded)
          Container(
            margin: const EdgeInsets.only(top: 6),
            padding: const EdgeInsets.fromLTRB(12, 10, 12, 12),
            decoration: BoxDecoration(
              color: Tokens.surface,
              borderRadius: BorderRadius.circular(Tokens.radius),
              border: Border.all(color: failed ? Tokens.negative : Tokens.border),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Row(children: [
                  Expanded(child: Text(tool, style: TextStyle(fontSize: 12.5, color: Tokens.text2))),
                  if (showOutput)
                    CopyButton(
                      key: const Key('copy-output'),
                      text: output,
                      tooltip: 'Copy output',
                      fetch: outputCut ? () async => (await _fetch())?.output : null,
                    ),
                ]),
                if (code.isNotEmpty) ...[
                  const SizedBox(height: 8),
                  Container(
                    padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
                    decoration: BoxDecoration(color: Tokens.bg, borderRadius: BorderRadius.circular(Tokens.radius)),
                    child: SelectableText('> $code', style: TextStyle(fontFamily: Tokens.mono, fontSize: 12, height: 1.45, color: Tokens.text)),
                  ),
                ],
                if (showOutput) ...[
                  const SizedBox(height: 8),
                  SingleChildScrollView(
                    scrollDirection: Axis.horizontal,
                    child: SelectableText(lines.take(shown).join('\n'), style: TextStyle(fontFamily: Tokens.mono, fontSize: 11.5, height: 1.4, color: Tokens.text2)),
                  ),
                  if (more > 0) Padding(padding: const EdgeInsets.only(top: 4), child: Text('… $more more lines', style: TextStyle(fontSize: 11, color: Tokens.muted))),
                ],
                if (cut) ...[
                  const SizedBox(height: 6),
                  Row(children: [
                    Expanded(
                      child: Text(
                        outputCut ? 'Shortened: showing ${byteSize(utf8.encode(a.output ?? '').length)} of ${byteSize(a.outputBytes!)}' : 'Shortened',
                        key: const Key('activity-shortened'),
                        style: TextStyle(fontSize: 11, color: Tokens.muted),
                      ),
                    ),
                    if (_fetching != null)
                      const SizedBox.square(dimension: 14, child: CircularProgressIndicator(strokeWidth: 1.5))
                    else
                      TextButton(
                        key: const Key('show-full-output'),
                        style: TextButton.styleFrom(visualDensity: VisualDensity.compact),
                        onPressed: _fetch,
                        child: Text(outputCut ? 'Show full output' : 'Show in full'),
                      ),
                  ]),
                ],
              ],
            ),
          ),
      ],
    );
  }
}

/// Input strings longer than this many bytes come shortened (`activity_preview`).
const _inputCap = 1000;

/// A byte count in words: "812 B", "2 KB", "7.8 KB", "1.2 MB".
String byteSize(int bytes) {
  String round(double v) => v >= 10 || v == v.roundToDouble() ? '${v.round()}' : v.toStringAsFixed(1);
  if (bytes < 1024) return '$bytes B';
  if (bytes < 1048576) return '${round(bytes / 1024)} KB';
  return '${round(bytes / 1048576)} MB';
}

/// "Links: [{title, url}, …]" from a web search result as "title — site" lines.
String webSearchSummary(String output) {
  final start = output.indexOf('Links: [');
  if (start >= 0) {
    final json = output.substring(start + 'Links: '.length);
    final end = json.indexOf('}]');
    if (end >= 0) {
      try {
        final links = (jsonDecode(json.substring(0, end + 2)) as List).cast<Map<String, dynamic>>();
        if (links.isNotEmpty) {
          return links.map((l) {
            final url = l['url'] as String? ?? '';
            final parts = url.split('/');
            final site = (parts.length > 2 ? parts[2] : url).replaceFirst(RegExp('^www\\.'), '');
            return '• ${l['title'] as String? ?? url} — $site';
          }).join('\n');
        }
      } on Object {
        // Not the expected shape: show the output as it is.
      }
    }
  }
  return output;
}
