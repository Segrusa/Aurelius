import 'package:intl/intl.dart';

import '../../data/run_config.dart';
import '../../data/views.dart';

// The pure rules of the session screen (design "Aurelius sessiescherm",
// Nov 2026): one screen for a work session and a ticket, its glass head, the
// answers' footer, the edit diffs and the Properties sheet.

/// What the screen shows: a work session or a ticket. Only the head's pill differs.
enum SessionKind {
  session('Session'),
  ticket('Ticket');

  const SessionKind(this.label);

  /// The pill under the title.
  final String label;
}

/// "Orbit · Opus 5.5": who answered and on which model, beside an answer's
/// Copy button. [producedBy] is the server's "Connection · model"; only its
/// model is shown, in words. Empty when neither is known.
String answerFooter({String? author, String? producedBy}) {
  final who = author?.trim() ?? '';
  final produced = producedBy?.trim() ?? '';
  final model = produced.isEmpty ? '' : ModelCatalog.friendly(produced.split(' · ').last);
  return [if (who.isNotEmpty) who, if (model.isNotEmpty) model].join(' · ');
}

/// The tools that write a file, as the desktop's `filesWritten`.
const _writing = {'Write', 'Edit', 'MultiEdit', 'NotebookEdit'};

/// The files the session's runs wrote, each once, in the order first written;
/// a failed write does not count (desktop: Files written).
List<String> filesWritten(Iterable<MessageView> messages) {
  final out = <String>{};
  for (final m in messages) {
    for (final a in m.activity) {
      final path = a.field(a.tool == 'NotebookEdit' ? 'notebook_path' : 'file_path');
      if (_writing.contains(a.tool.replaceFirst('claude.', '')) && a.ok != false && path.isNotEmpty) out.add(path);
    }
  }
  return out.toList();
}

/// The run of the latest message that has one: its guidelines are the ones in force.
String? lastRunOf(List<MessageView> messages) => messages.reversed.where((m) => m.runId != null).firstOrNull?.runId;

/// "20-11-2026, 13:32": when the session started, as the design shows it.
String startedLabel(int ms) => DateFormat('dd-MM-yyyy, HH:mm').format(DateTime.fromMillisecondsSinceEpoch(ms));

/// One line of an edit's diff card: removed (`-`) or added (`+`).
class DiffLine {
  const DiffLine(this.added, this.text);

  final bool added;
  final String text;

  @override
  bool operator ==(Object other) => other is DiffLine && other.added == added && other.text == text;

  @override
  int get hashCode => Object.hash(added, text);

  @override
  String toString() => '${added ? '+' : '-'} $text';
}

/// At most this many lines in a diff card; the rest is counted.
const diffLinesMax = 40;

/// The removed and added lines of an Edit, MultiEdit or Write, as the diff card
/// shows them: each edit's old lines, then its new ones. Empty for other tools.
List<DiffLine> editDiff(ActivityView a) {
  List<String> lines(Object? text) => text is String && text.isNotEmpty ? text.split('\n') : const [];
  Iterable<DiffLine> edit(Object? before, Object? after) => [
        for (final l in lines(before)) DiffLine(false, l),
        for (final l in lines(after)) DiffLine(true, l),
      ];
  return switch (a.tool) {
    'Edit' => edit(a.inputValue('old_string'), a.inputValue('new_string')).toList(),
    'MultiEdit' => [
        for (final e in a.inputValue('edits') is List ? a.inputValue('edits') as List : const [])
          if (e is Map) ...edit(e['old_string'], e['new_string']),
      ],
    'Write' => edit(null, a.inputValue('content')).toList(),
    _ => const [],
  };
}

/// The diff card's title: the file's last two path parts ("widgets/ios.dart").
String diffTitle(String path) {
  final parts = path.split(RegExp(r'[\\/]')).where((p) => p.isNotEmpty).toList();
  return parts.length <= 2 ? parts.join('/') : parts.sublist(parts.length - 2).join('/');
}

/// How the Status chip in the Properties sheet looks.
enum StatusTone { working, waiting }

/// A session works while a run is queued or under way; otherwise it waits (for you or a choice).
StatusTone sessionTone(bool running, SessionDispatch? dispatch) => dispatch == null && running ? StatusTone.working : StatusTone.waiting;
